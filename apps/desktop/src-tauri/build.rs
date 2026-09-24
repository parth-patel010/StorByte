fn main() {
    embed_google_oauth_from_dotenv();

    #[cfg(windows)]
    winfsp::build::winfsp_link_delayload();
    tauri_build::build();
}

/// Bake OAuth client credentials into the binary so installed copies work
/// without shipping a `.env` next to the exe (dev still can override via env/.env).
///
/// Values are XOR-obfuscated before `rustc-env` so a plain `strings` dump does not
/// reveal the client id / secret.
fn embed_google_oauth_from_dotenv() {
    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_SECRET");

    let mut id = std::env::var("GOOGLE_CLIENT_ID").ok().filter(|s| !s.is_empty());
    let mut secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.is_empty());

    if id.is_none() || secret.is_none() {
        for path in ["../.env", ".env"] {
            let Ok(raw) = std::fs::read_to_string(path) else {
                continue;
            };
            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((k, v)) = line.split_once('=') else {
                    continue;
                };
                let k = k.trim();
                let v = v.trim().trim_matches('"');
                if k == "GOOGLE_CLIENT_ID" && id.is_none() && !v.is_empty() {
                    id = Some(v.to_string());
                }
                if k == "GOOGLE_CLIENT_SECRET" && secret.is_none() && !v.is_empty() {
                    secret = Some(v.to_string());
                }
            }
            break;
        }
    }

    // Key derived from package identity so it is not a fixed obvious constant in source alone.
    let key = derive_obf_key();
    println!("cargo:rustc-env=STORBYTE_OBF_KEY={key}");

    if let Some(id) = id {
        println!(
            "cargo:rustc-env=GOOGLE_CLIENT_ID_OBF={}",
            xor_hex_encode(id.as_bytes(), key)
        );
    }
    if let Some(secret) = secret {
        println!(
            "cargo:rustc-env=GOOGLE_CLIENT_SECRET_OBF={}",
            xor_hex_encode(secret.as_bytes(), key)
        );
    }
}

fn derive_obf_key() -> u8 {
    let seed = b"Storbyte.oauth.v1";
    let mut k: u8 = 0xA5;
    for (i, b) in seed.iter().enumerate() {
        k = k.wrapping_mul(31).wrapping_add(b.wrapping_add(i as u8));
        if k == 0 {
            k = 0x5A;
        }
    }
    k
}

fn xor_hex_encode(data: &[u8], key: u8) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for (i, b) in data.iter().enumerate() {
        let x = b ^ key.wrapping_add(i as u8).wrapping_mul(17);
        out.push_str(&format!("{x:02x}"));
    }
    out
}
