//! The session `.zip` bundle.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use statasmon_store::sanitize;
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

use crate::progress::Progress;
use crate::writers;
use crate::{ExportError, Job};

/// Directories that may hold crash reports for a device.
fn crash_dirs(app_data: &Path, device_id: &str) -> Vec<PathBuf> {
    let base = app_data.join("crashes");
    let mut dirs = vec![base.join(sanitize(device_id))];
    let raw = base.join(device_id);
    if raw != dirs[0] {
        dirs.push(raw);
    }
    dirs
}

/// Files (not directories) directly inside `dir`, sorted by name.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    files
}

/// Write the bundle into `out`; returns the number of data rows written
/// (samples + cores + processes + log entries).
pub(crate) fn write_bundle(
    job: &Job<'_>,
    out: &mut BufWriter<File>,
    p: &mut Progress<'_>,
) -> Result<u64, ExportError> {
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let summary = job.store.get(job.session_id)?;
    let info = job.store.info(job.session_id)?;
    p.set_total(job.log_total()?);
    let mut zip = ZipWriter::new(out);

    zip.start_file("device.json", opts)?;
    serde_json::to_writer_pretty(
        &mut zip,
        &serde_json::json!({
            "id": summary.device_id,
            "name": summary.device_name,
            "model_name": summary.model_name,
            "platform": summary.platform,
            "info": info,
        }),
    )?;

    zip.start_file("session.json", opts)?;
    serde_json::to_writer_pretty(
        &mut zip,
        &serde_json::json!({
            "statasmon": 1,
            "protocol_version": statasmon_protocol::PROTOCOL_VERSION,
            "exported_at_ms": statasmon_store::now_ms(),
            "scope": job.request.scope,
            "session": summary,
            "meta": job.reader.meta()?,
        }),
    )?;

    let mut entries = 0u64;
    zip.start_file("metrics.csv", opts)?;
    entries += writers::write_metrics_wide(job, &mut zip, p)?;
    zip.start_file("cores.csv", opts)?;
    entries += writers::write_cores_csv(job, &mut zip, p)?;
    zip.start_file("processes.csv", opts)?;
    entries += writers::write_processes_csv(job, &mut zip, p, None)?;

    zip.start_file("logs.jsonl", opts)?;
    serde_json::to_writer(&mut zip, &writers::jsonl_header(job)?)?;
    zip.write_all(b"\n")?;
    entries += writers::for_each_log(job, |e| {
        serde_json::to_writer(&mut zip, e)?;
        zip.write_all(b"\n")?;
        p.tick(1, "Writing logs");
        Ok(())
    })?;

    zip.start_file("events.json", opts)?;
    serde_json::to_writer_pretty(&mut zip, &job.reader.events()?)?;
    zip.start_file("marks.json", opts)?;
    serde_json::to_writer_pretty(&mut zip, &job.reader.marks()?)?;

    if let Some(app_data) = job.app_data_dir {
        let mut seen = std::collections::BTreeSet::new();
        for dir in crash_dirs(app_data, &summary.device_id) {
            for file in files_in(&dir) {
                let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !seen.insert(name.to_owned()) {
                    continue;
                }
                zip.start_file(format!("crashes/{name}"), opts)?;
                let mut f = File::open(&file)?;
                std::io::copy(&mut f, &mut zip)?;
            }
        }
        let pcap = app_data
            .join("pcap")
            .join(format!("{}.pcap", job.session_id));
        if pcap.is_file() {
            zip.start_file("capture.pcap", opts)?;
            let mut f = File::open(&pcap)?;
            std::io::copy(&mut f, &mut zip)?;
        }
    }
    zip.finish()?;
    Ok(entries)
}
