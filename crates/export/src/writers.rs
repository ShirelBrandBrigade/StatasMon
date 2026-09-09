//! Line and row writers shared by the single-file formats and the bundle.

use std::collections::HashMap;
use std::io::Write;

use chrono::{Local, TimeZone};
use statasmon_protocol::{ExportScope, LogEntry, LogLevel, MetricId};
use statasmon_store::{StoreError, Table};

use crate::progress::Progress;
use crate::{ExportError, Job, LogSource, WIDE_MAX_METRICS};

/// Wrap a non-store error so it can travel through a store iterator.
fn io_err<E: std::error::Error + Send + Sync + 'static>(e: E) -> StoreError {
    StoreError::Io(std::io::Error::other(e))
}

/// "2026-09-09 14:32:07.412" in local time.
pub(crate) fn local_stamp(ts_ms: i64) -> String {
    Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
        .unwrap_or_else(|| ts_ms.to_string())
}

fn level_upper(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Notice => "NOTICE",
        LogLevel::Warning => "WARNING",
        LogLevel::Error => "ERROR",
        LogLevel::Fault => "FAULT",
    }
}

/// One `.log` line (with its newline).
pub(crate) fn log_line(e: &LogEntry) -> String {
    let process = match e.pid {
        Some(pid) => format!("{}[{pid}]", e.process),
        None => e.process.clone(),
    };
    let origin = e
        .subsystem
        .as_deref()
        .or(e.tag.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("-");
    format!(
        "{}  {}  {}  {}  {}\n",
        local_stamp(e.ts_ms),
        process,
        level_upper(e.level),
        origin,
        e.message.replace('\n', "\\n")
    )
}

/// The first line of a `.jsonl` export.
pub(crate) fn jsonl_header(job: &Job<'_>) -> Result<serde_json::Value, ExportError> {
    let summary = job.store.get(job.session_id)?;
    Ok(serde_json::json!({
        "statasmon": 1,
        "protocol_version": statasmon_protocol::PROTOCOL_VERSION,
        "exported_at_ms": statasmon_store::now_ms(),
        "session": summary,
        "scope": job.request.scope,
    }))
}

/// Stream every log entry of the job's scope, oldest first.
pub(crate) fn for_each_log(
    job: &Job<'_>,
    mut f: impl FnMut(&LogEntry) -> Result<(), ExportError>,
) -> Result<u64, ExportError> {
    match &job.logs {
        LogSource::Filter(filter) => {
            let mut failure: Option<ExportError> = None;
            let n = job.reader.for_each_log(filter, |entry| {
                if let Err(e) = f(&entry) {
                    failure = Some(e);
                    return Err(io_err(std::io::Error::other("export aborted")));
                }
                Ok(())
            });
            if let Some(e) = failure {
                return Err(e);
            }
            Ok(n?)
        }
        LogSource::Seqs(seqs) => {
            let entries = job.reader.logs_by_seq(seqs)?;
            for e in &entries {
                f(e)?;
            }
            Ok(entries.len() as u64)
        }
    }
}

/// `.log`: one line per entry.
pub(crate) fn write_log(
    job: &Job<'_>,
    out: &mut dyn Write,
    p: &mut Progress<'_>,
) -> Result<u64, ExportError> {
    p.set_total(job.log_total()?);
    for_each_log(job, |e| {
        out.write_all(log_line(e).as_bytes())?;
        p.tick(1, "Writing log");
        Ok(())
    })
}

/// `.jsonl`: header line then one entry per line.
pub(crate) fn write_jsonl(
    job: &Job<'_>,
    out: &mut dyn Write,
    p: &mut Progress<'_>,
) -> Result<u64, ExportError> {
    p.set_total(job.log_total()?);
    serde_json::to_writer(&mut *out, &jsonl_header(job)?)?;
    out.write_all(b"\n")?;
    for_each_log(job, |e| {
        serde_json::to_writer(&mut *out, e)?;
        out.write_all(b"\n")?;
        p.tick(1, "Writing log");
        Ok(())
    })
}

/// Shortest exact decimal for a stored value.
pub(crate) fn fmt_value(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// `.csv`: wide metrics for a whole session with ≤ 40 metrics, process
/// rows for the `App` scope, long metrics otherwise.
pub(crate) fn write_csv(
    job: &Job<'_>,
    out: &mut dyn Write,
    p: &mut Progress<'_>,
) -> Result<u64, ExportError> {
    match &job.request.scope {
        ExportScope::App { app_id } => write_processes_csv(job, out, p, Some(app_id)),
        ExportScope::Session if job.reader.metrics()?.len() <= WIDE_MAX_METRICS => {
            write_metrics_wide(job, out, p)
        }
        _ => write_metrics_long(job, out, p),
    }
}

/// `ts_ms,cpu.total,mem.used,…` — one row per sample tick.
pub(crate) fn write_metrics_wide<W: Write>(
    job: &Job<'_>,
    out: W,
    p: &mut Progress<'_>,
) -> Result<u64, ExportError> {
    let metrics = job.reader.metrics()?;
    p.add_total(job.rows(Table::Samples)?);
    let mut w = csv::Writer::from_writer(out);
    let mut header = vec!["ts_ms".to_owned()];
    header.extend(metrics.iter().map(|m| m.as_str().to_owned()));
    w.write_record(&header)?;
    let index: HashMap<MetricId, usize> =
        metrics.iter().enumerate().map(|(i, m)| (*m, i)).collect();
    let mut current: Option<i64> = None;
    let mut row: Vec<String> = vec![String::new(); metrics.len()];
    let mut rows = 0u64;
    let flush_row =
        |w: &mut csv::Writer<W>, ts: i64, row: &mut Vec<String>| -> Result<(), csv::Error> {
            let mut record = Vec::with_capacity(row.len() + 1);
            record.push(ts.to_string());
            record.extend(row.iter().cloned());
            w.write_record(&record)?;
            for cell in row.iter_mut() {
                cell.clear();
            }
            Ok(())
        };
    job.reader
        .for_each_sample(job.from_ms, job.to_ms, |ts, metric, value| {
            if current != Some(ts) {
                if let Some(t) = current {
                    flush_row(&mut w, t, &mut row).map_err(io_err)?;
                    rows += 1;
                }
                current = Some(ts);
            }
            if let Some(i) = index.get(&metric) {
                row[*i] = fmt_value(value);
            }
            p.tick(1, "Writing metrics");
            Ok(())
        })?;
    if let Some(t) = current {
        flush_row(&mut w, t, &mut row)?;
        rows += 1;
    }
    w.flush()?;
    Ok(rows)
}

/// `ts_ms,metric,value` — one row per sample.
pub(crate) fn write_metrics_long<W: Write>(
    job: &Job<'_>,
    out: W,
    p: &mut Progress<'_>,
) -> Result<u64, ExportError> {
    p.add_total(job.rows(Table::Samples)?);
    let mut w = csv::Writer::from_writer(out);
    w.write_record(["ts_ms", "metric", "value"])?;
    let n = job
        .reader
        .for_each_sample(job.from_ms, job.to_ms, |ts, metric, value| {
            w.write_record([ts.to_string(), metric.as_str().to_owned(), fmt_value(value)])
                .map_err(io_err)?;
            p.tick(1, "Writing metrics");
            Ok(())
        })?;
    w.flush()?;
    Ok(n)
}

/// `ts_ms,index,kind,usage_pct,freq_mhz,online`.
pub(crate) fn write_cores_csv<W: Write>(
    job: &Job<'_>,
    out: W,
    p: &mut Progress<'_>,
) -> Result<u64, ExportError> {
    p.add_total(job.rows(Table::Cores)?);
    let mut w = csv::Writer::from_writer(out);
    w.write_record(["ts_ms", "index", "kind", "usage_pct", "freq_mhz", "online"])?;
    let n = job.reader.for_each_core(job.from_ms, job.to_ms, |ts, c| {
        w.write_record([
            ts.to_string(),
            c.index.to_string(),
            statasmon_store::codec::core_kind_name(c.kind),
            fmt_value(f64::from(c.usage_pct)),
            c.freq_mhz.map(|f| f.to_string()).unwrap_or_default(),
            if c.online { "1" } else { "0" }.to_owned(),
        ])
        .map_err(io_err)?;
        p.tick(1, "Writing cores");
        Ok(())
    })?;
    w.flush()?;
    Ok(n)
}

/// `ts_ms,pid,name,app_id,cpu_pct,mem_bytes,threads,net_bps,state`.
pub(crate) fn write_processes_csv<W: Write>(
    job: &Job<'_>,
    out: W,
    p: &mut Progress<'_>,
    app_id: Option<&str>,
) -> Result<u64, ExportError> {
    p.add_total(job.rows(Table::Processes)?);
    let mut w = csv::Writer::from_writer(out);
    w.write_record([
        "ts_ms",
        "pid",
        "name",
        "app_id",
        "cpu_pct",
        "mem_bytes",
        "threads",
        "net_bps",
        "state",
    ])?;
    let n = job
        .reader
        .for_each_process(job.from_ms, job.to_ms, app_id, |ts, r| {
            w.write_record([
                ts.to_string(),
                r.pid.to_string(),
                r.name.clone(),
                r.app_id.clone().unwrap_or_default(),
                r.cpu_pct
                    .map(|v| fmt_value(f64::from(v)))
                    .unwrap_or_default(),
                r.mem_bytes.map(|v| v.to_string()).unwrap_or_default(),
                r.threads.map(|v| v.to_string()).unwrap_or_default(),
                r.net_bps.map(fmt_value).unwrap_or_default(),
                statasmon_store::codec::process_state_name(r.state),
            ])
            .map_err(io_err)?;
            p.tick(1, "Writing processes");
            Ok(())
        })?;
    w.flush()?;
    Ok(n)
}
