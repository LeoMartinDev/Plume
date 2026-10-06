#[cfg(test)]
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::{io, Release, Result};

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
    let expected = parse_checksum(&checksum, &release.installer_name)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30 * 60)))
        .timeout_recv_response(Some(std::time::Duration::from_secs(30)))
        .build()
        .into();
    let mut response = agent
        .get(&release.installer_url)
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checksum_must_match_the_selected_asset() {
        let hash = "a".repeat(64);
        assert!(parse_checksum(&format!("{hash}  app.exe\n"), "app.exe").is_ok());
        for text in [
            format!("{hash} other.exe"),
            "abc app.exe".into(),
            format!("{hash} app.exe\n{hash} other.exe"),
        ] {
            assert!(parse_checksum(&text, "app.exe").is_err());
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
    fn corrupt_download_is_rejected_before_installation() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let checksum = format!("{:x}  app.exe\n", Sha256::digest(b"good"));
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
            installer_name: "app.exe".into(),
            installer_url: format!("{base}/app.exe"),
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
