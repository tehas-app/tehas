//! The host's models.json as a command, the rules being tehas_core::models.
//!
//!   console models keys-ok
//!     every key is a word or a set TEHAS_MODEL_KEY_ name; else 78
//!   console models pick <provider>
//!     "main" or "backup": main when it offers the provider's models, else
//!     backup when it does, else main. No call without a backupBaseUrl
//!   console models resolve <out>
//!     write the file to <out> with each provider on its picked endpoint, and
//!     print "<provider> backup" for each on one
//!
//! Exit: 0 done; 1 the file cannot be used; 64 bad words; 78 a bad key.

use std::path::Path;
use tehas_core::models::{self, ModelFile};

pub fn main(given: &[String]) -> u8 {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["keys-ok"] => match models::host_keys_ok() {
            Ok(()) => 0,
            Err(why) => {
                eprintln!("tehas: {why}");
                78
            }
        },
        ["pick", provider] => {
            let file = ModelFile::read(&models::host_path());
            println!(
                "{}",
                models::pick(&file, provider, &models::used(), &models::curl)
            );
            0
        }
        ["resolve", out] => match models::write(Path::new(out)) {
            Some(written) => {
                for name in written.moved {
                    println!("{name} backup");
                }
                0
            }
            None => 1,
        },
        _ => {
            eprintln!("usage: console models keys-ok | pick <provider> | resolve <out>");
            64
        }
    }
}
