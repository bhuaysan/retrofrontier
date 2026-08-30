//! Maintainer tool that constructs and publishes a managed Runtime Release.
//!
//! Usage:
//!
//! ```text
//! rf-runtime-release build   --definition <file> --output <dir> [--cache <dir>] [--offline]
//! rf-runtime-release publish --definition <file> --output <dir> --keys <dir> [--cache <dir>] [--offline]
//! rf-runtime-release pin     --definition <file> [--cache <dir>] [--offline]
//! ```
//!
//! `build` verifies pinned inputs, derives every component artefact, emits the canonical release
//! manifest and runtime policy, and proves the manifest by extracting it through the real client
//! extractor. `publish` additionally signs a TUF 1.0 repository. `pin` reports the digests and
//! lengths a maintainer needs when first introducing or refreshing a definition, and never edits
//! the definition itself, so a pin change is always a reviewed commit.
//!
//! No network access happens without a pinned HTTPS URL, and no downloaded byte is used before its
//! length and SHA-256 match the definition.

use retrofrontier_lib::release::construct::{construct_release, InputCache};
use retrofrontier_lib::release::tuf::{publish_release, KeyDirectory};
use std::path::PathBuf;
use std::process::ExitCode;

struct Arguments {
    command: String,
    definition: PathBuf,
    output: PathBuf,
    cache: PathBuf,
    keys: Option<PathBuf>,
    allow_download: bool,
}

fn usage() -> &'static str {
    "usage: rf-runtime-release <build|publish|pin> --definition <file> [--output <dir>] \
     [--cache <dir>] [--keys <dir>] [--offline]"
}

fn parse_arguments() -> Result<Arguments, String> {
    let mut raw = std::env::args().skip(1);
    let command = raw.next().ok_or_else(|| usage().to_owned())?;
    if !matches!(command.as_str(), "build" | "publish" | "pin") {
        return Err(usage().to_owned());
    }
    let mut definition = None;
    let mut output = None;
    let mut cache = None;
    let mut keys = None;
    let mut allow_download = true;
    while let Some(flag) = raw.next() {
        let mut value = || {
            raw.next()
                .ok_or_else(|| format!("{flag} requires a value"))
                .map(PathBuf::from)
        };
        match flag.as_str() {
            "--definition" => definition = Some(value()?),
            "--output" => output = Some(value()?),
            "--cache" => cache = Some(value()?),
            "--keys" => keys = Some(value()?),
            "--offline" => allow_download = false,
            other => return Err(format!("unknown argument '{other}'\n{}", usage())),
        }
    }
    let definition = definition.ok_or_else(|| "--definition is required".to_owned())?;
    let output = output.unwrap_or_else(|| PathBuf::from("target/runtime-release"));
    let cache = cache.unwrap_or_else(|| output.join("input-cache"));
    if command == "publish" && keys.is_none() {
        return Err("--keys is required for publish".to_owned());
    }
    Ok(Arguments {
        command,
        definition,
        output,
        cache,
        keys,
        allow_download,
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let arguments = match parse_arguments() {
        Ok(arguments) => arguments,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    match run(arguments).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("release construction failed: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run(arguments: Arguments) -> Result<(), String> {
    let cache = InputCache::new(arguments.cache.clone(), arguments.allow_download);

    if arguments.command == "pin" {
        return report_pins(&arguments, &cache).await;
    }

    let release = construct_release(&arguments.definition, &arguments.output, &cache)
        .await
        .map_err(|error| error.to_string())?;

    println!("release       {}", release.manifest.release.release_id);
    println!(
        "sequence      {}",
        release.manifest.release.release_sequence
    );
    println!(
        "retroarch     {}",
        release.manifest.release.retroarch_version
    );
    println!("manifest      {}", release.manifest_target_name);
    println!("manifest hash {}", release.manifest_sha256.to_hex());
    println!(
        "inventory     {} entries",
        release.manifest.release.inventory.len()
    );
    for target in &release.targets {
        println!(
            "target        {:<52} {:>12}  {}",
            target.name,
            target.size_bytes,
            target.sha256.to_hex()
        );
    }

    if arguments.command == "publish" {
        let keys = KeyDirectory::new(arguments.keys.clone().expect("checked during parsing"));
        let published = publish_release(&release, &arguments.output, &keys)
            .await
            .map_err(|error| error.to_string())?;
        println!("metadata      {}", published.metadata_directory.display());
        println!("targets       {}", published.targets_directory.display());
        println!("trusted root  {}", published.root_json.display());
    }
    Ok(())
}

/// Report the digests a maintainer needs to write a definition, without mutating it.
async fn report_pins(arguments: &Arguments, cache: &InputCache) -> Result<(), String> {
    // Constructing with intentionally wrong pins is not useful, so `pin` reports what it sees on
    // disk in the input cache. A maintainer downloads once, inspects provenance, and commits the
    // values by hand.
    let entries = std::fs::read_dir(&arguments.cache)
        .map_err(|error| format!("input cache is unreadable: {error}"))?;
    let _ = cache;
    let mut any = false;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|error| error.to_string())?;
        let digest = retrofrontier_lib::release::canonical::sha256_hex(&bytes);
        println!(
            "{:<36} {:>12}  {}",
            entry.file_name().to_string_lossy(),
            bytes.len(),
            digest
        );
        any = true;
    }
    if !any {
        return Err(format!(
            "no cached inputs found in {}",
            arguments.cache.display()
        ));
    }
    Ok(())
}
