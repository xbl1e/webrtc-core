#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let sdp_str = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    let parsed_sdp = parse_sdp_minimal(sdp_str);

    if let Some(sdp) = parsed_sdp {
        assert!(!sdp.version.is_empty());
        assert!(!sdp.media.is_empty());
    }
});

#[derive(Debug, Default)]
struct MinimalSdp {
    version: String,
    origin: String,
    session_name: String,
    connection: String,
    media: Vec<MediaSection>,
}

#[derive(Debug, Default)]
struct MediaSection {
    media_type: String,
    port: u16,
    protocol: String,
    formats: String,
    attributes: Vec<String>,
}

fn parse_sdp_minimal(input: &str) -> Option<MinimalSdp> {
    let mut sdp = MinimalSdp::default();
    let mut current_media = None;

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            continue;
        }

        let key = parts[0].trim();
        let value = parts[1].trim();

        match key {
            "v" => sdp.version = value.to_string(),
            "o" => sdp.origin = value.to_string(),
            "s" => sdp.session_name = value.to_string(),
            "c" => sdp.connection = value.to_string(),
            "m" => {
                if let Some(media) = current_media.take() {
                    sdp.media.push(media);
                }
                let media_parts: Vec<&str> = value.split_whitespace().collect();
                if media_parts.len() >= 3 {
                    let port = media_parts.get(1)
                        .and_then(|p| p.parse::<u16>().ok())
                        .unwrap_or(0);

                    current_media = Some(MediaSection {
                        media_type: media_parts.get(0).unwrap_or(&"").to_string(),
                        port,
                        protocol: media_parts.get(2).unwrap_or(&"").to_string(),
                        formats: media_parts[3..].join(" "),
                        attributes: Vec::new(),
                    });
                }
            }
            "a" => {
                if let Some(ref mut media) = current_media {
                    media.attributes.push(value.to_string());
                }
            }
            _ => {}
        }
    }

    if let Some(media) = current_media {
        sdp.media.push(media);
    }

    if sdp.version.is_empty() || sdp.media.is_empty() {
        return None;
    }

    Some(sdp)
}
