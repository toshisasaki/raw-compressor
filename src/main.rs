use clap::{Arg, Command};
use log::{error, info};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use walkdir::WalkDir;
use xz2::write::XzEncoder;

fn main() {
    pretty_env_logger::init();
    info!("Starting raw-compressor");

    let matches = Command::new("raw-compressor")
        .arg(
            Arg::new("input")
                .long("input")
                .value_parser(clap::builder::ValueParser::path_buf())
                .required(true)
                .help("Input directory to traverse"),
        )
        .arg(
            Arg::new("originals")
                .long("originals")
                .value_parser(clap::builder::ValueParser::path_buf())
                .required(true)
                .help("Directory to move original files to"),
        )
        .get_matches();

    let input_dir = matches.get_one::<PathBuf>("input").unwrap();
    let originals_dir = matches.get_one::<PathBuf>("originals").unwrap();
    let exts = ["cr3", "raw", "nef"];

    fs::create_dir_all(originals_dir).unwrap_or_else(|e| {
        error!("Failed to create originals dir: {}", e);
        std::process::exit(1);
    });

    // Collect all matching files first
    let files: Vec<PathBuf> = WalkDir::new(input_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(OsStr::to_str)
                    .map(|ext| exts.iter().any(|&x| ext.eq_ignore_ascii_case(x)))
                    .unwrap_or(false)
        })
        .map(|entry| entry.path().to_path_buf())
        .collect();

    // Process files in parallel
    files.par_iter().for_each(|path| {
        let ext = path.extension().and_then(OsStr::to_str).unwrap();
        let compressed_path =
            get_unique_path(path.with_extension(format!("{}.xz", ext)));
        info!("Compressing file: {:?} -> {:?}", path, compressed_path);
        let input_file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open input file {:?}: {}", path, e);
                return;
            }
        };
        let output_file = match File::create(&compressed_path) {
            Ok(f) => f,
            Err(e) => {
                error!(
                    "Failed to create compressed file {:?}: {}",
                    compressed_path, e
                );
                return;
            }
        };
        let mut encoder = XzEncoder::new(BufWriter::new(output_file), 6);
        if let Err(e) = std::io::copy(&mut BufReader::new(input_file), &mut encoder) {
            error!("Compression failed for {:?}: {}", path, e);
            return;
        }
        if let Err(e) = encoder.finish() {
            error!("Failed to finish compression for {:?}: {}", path, e);
            return;
        }
        // Move original file
        let original_name = path.file_name().unwrap();
        let mut target_path = originals_dir.clone();
        target_path.push(original_name);
        let target_path = get_unique_path(target_path);
        info!("Moving original file: {:?} -> {:?}", path, target_path);
        if let Err(e) = fs::rename(path, &target_path) {
            error!(
                "Failed to move original file {:?} to {:?}: {}",
                path, target_path, e
            );
        }
    });
}

fn get_unique_path(path: PathBuf) -> PathBuf {
    let orig_stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("");
    let ext = path.extension().and_then(OsStr::to_str);
    let mut counter = 1;
    let mut candidate = path.clone();
    while candidate.exists() {
        let new_stem = format!("{}-{}", orig_stem, counter);
        let new_name = match ext {
            Some(e) => format!("{}.{}", new_stem, e),
            None => new_stem,
        };
        candidate.set_file_name(new_name);
        counter += 1;
    }
    candidate
}
