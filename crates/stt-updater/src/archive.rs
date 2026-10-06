use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{io, Release, Result};

const MAX_EXPANDED: u64 = 4 * 1024 * 1024 * 1024;

pub(crate) fn download(
    release: &Release,
    destination: &Path,
    progress: &impl Fn(u64, u64),
) -> Result<()> {
    let mut checksum_response = crate::github::agent()
        .get(&release.checksum_url)
        .call()
        .map_err(|e| e.to_string())?;
    let checksum = checksum_response
        .body_mut()
        .with_config()
        .limit(4096)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let expected = parse_checksum(&checksum, &release.archive_name)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30 * 60)))
        .timeout_recv_response(Some(std::time::Duration::from_secs(30)))
        .build()
        .into();
    let mut response = agent
        .get(&release.archive_url)
        .call()
        .map_err(|e| e.to_string())?;
    let mut reader = response.body_mut().as_reader();
    let mut file = io(File::create(destination))?;
    let mut hash = Sha256::new();
    let mut received = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = io(reader.read(&mut buffer))?;
        if count == 0 {
            break;
        }
        received += count as u64;
        if received > release.size {
            return Err("Update download exceeded its expected size.".into());
        }
        hash.update(&buffer[..count]);
        io(file.write_all(&buffer[..count]))?;
        progress(received, release.size);
    }
    io(file.sync_all())?;
    if received != release.size || format!("{:x}", hash.finalize()) != expected {
        return Err("Update verification failed. Your installed app has not been changed.".into());
    }
    Ok(())
}

fn parse_checksum(text: &str, filename: &str) -> Result<String> {
    let parts: Vec<_> = text.split_whitespace().collect();
    if parts.len() != 2
        || parts[1].trim_start_matches('*') != filename
        || parts[0].len() != 64
        || !parts[0].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Invalid update checksum file.".into());
    }
    Ok(parts[0].to_ascii_lowercase())
}

fn safe_path(path: &Path, root: &str) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(name) => result.push(name),
            Component::CurDir => {}
            _ => return Err("Unsafe path in update archive.".into()),
        }
    }
    if result.components().next().map(|part| part.as_os_str()) != Some(std::ffi::OsStr::new(root)) {
        return Err("Unexpected folder in update archive.".into());
    }
    Ok(result)
}

pub(crate) fn extract(
    archive: &Path,
    destination: &Path,
    root: &str,
    windows: bool,
) -> Result<PathBuf> {
    io(fs::create_dir(destination))?;
    let mut total = 0u64;
    if windows {
        let mut zip = zip::ZipArchive::new(io(File::open(archive))?).map_err(|e| e.to_string())?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err("Links are not allowed in an update archive.".into());
            }
            let name = entry.enclosed_name().ok_or("Unsafe ZIP path")?;
            let output = destination.join(safe_path(&name, root)?);
            total = total
                .checked_add(entry.size())
                .ok_or("Update archive is too large")?;
            if total > MAX_EXPANDED {
                return Err("Update archive is too large".into());
            }
            if entry.is_dir() {
                io(fs::create_dir_all(output))?;
            } else {
                io(fs::create_dir_all(
                    output.parent().ok_or("Invalid archive path")?,
                ))?;
                let mut file = io(File::options().write(true).create_new(true).open(output))?;
                io(std::io::copy(&mut entry, &mut file))?;
            }
        }
    } else {
        let gzip = flate2::read::GzDecoder::new(io(File::open(archive))?);
        let mut tar = tar::Archive::new(gzip);
        for entry in io(tar.entries())? {
            let mut entry = io(entry)?;
            let kind = entry.header().entry_type();
            if !kind.is_file() && !kind.is_dir() {
                return Err("Links are not allowed in an update archive.".into());
            }
            let output = destination.join(safe_path(&io(entry.path())?, root)?);
            total = total
                .checked_add(entry.size())
                .ok_or("Update archive is too large")?;
            if total > MAX_EXPANDED {
                return Err("Update archive is too large".into());
            }
            if output.is_file() {
                return Err("Duplicate file in update archive.".into());
            }
            if let Some(parent) = output.parent() {
                io(fs::create_dir_all(parent))?;
            }
            io(entry.unpack(output))?;
        }
    }
    let extracted = destination.join(root);
    if !extracted.is_dir() {
        return Err("Update archive is empty.".into());
    }
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_absolute_paths_and_wrong_roots() {
        for path in [
            "../root/plume",
            "/root/plume",
            "root/../plume",
            "other/plume",
        ] {
            assert!(safe_path(Path::new(path), "root").is_err(), "{path}");
        }
        assert_eq!(
            safe_path(Path::new("root/Plume.app/Contents/MacOS/plume"), "root").unwrap(),
            PathBuf::from("root/Plume.app/Contents/MacOS/plume")
        );
    }
    #[test]
    fn checksum_must_match_the_selected_asset() {
        let hash = "a".repeat(64);
        assert!(parse_checksum(&format!("{hash}  app.zip\n"), "app.zip").is_ok());
        for text in [
            format!("{hash} other.zip"),
            "abc app.zip".into(),
            format!("{hash} app.zip\n{hash} other.zip"),
        ] {
            assert!(parse_checksum(&text, "app.zip").is_err());
        }
    }

    fn temp() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "plume-archive-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn extracts_native_archives_and_rejects_links() {
        let temp = temp();
        let archive = temp.join("app.tar.gz");
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            File::create(&archive).unwrap(),
            flate2::Compression::default(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                "root/Plume.app/Contents/MacOS/plume",
                &b"app"[..],
            )
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        let root = extract(&archive, &temp.join("tar"), "root", false).unwrap();
        assert_eq!(
            fs::read(root.join("Plume.app/Contents/MacOS/plume")).unwrap(),
            b"app"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(root.join("Plume.app/Contents/MacOS/plume"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            File::create(&archive).unwrap(),
            flate2::Compression::default(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name("../../outside").unwrap();
        header.set_cksum();
        builder
            .append_data(&mut header, "root/link", &b""[..])
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        assert!(extract(&archive, &temp.join("link"), "root", false).is_err());

        let archive = temp.join("app.zip");
        let mut builder = zip::ZipWriter::new(File::create(&archive).unwrap());
        builder
            .start_file("root/plume.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        builder.write_all(b"app").unwrap();
        builder.finish().unwrap();
        let root = extract(&archive, &temp.join("zip"), "root", true).unwrap();
        assert_eq!(fs::read(root.join("plume.exe")).unwrap(), b"app");
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn corrupt_download_is_rejected_before_installation() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let checksum = format!("{:x}  app.zip\n", Sha256::digest(b"good"));
        let server = std::thread::spawn(move || {
            for body in [checksum.as_bytes(), &b"evil"[..]] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let _ = stream.read(&mut request).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
        });
        let temp = temp();
        let release = Release {
            version: "0.2.0".into(),
            notes: String::new(),
            url: String::new(),
            archive_name: "app.zip".into(),
            archive_url: format!("{base}/app.zip"),
            checksum_url: format!("{base}/checksum"),
            size: 4,
        };
        assert!(download(&release, &temp.join("download"), &|_, _| {})
            .unwrap_err()
            .contains("verification failed"));
        server.join().unwrap();
        fs::remove_dir_all(temp).unwrap();
    }
}
