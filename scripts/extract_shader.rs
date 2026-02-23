#!/usr/bin/env rust-script
//! Extract terrain shaders from ERA archives
//! 
//! Usage: cargo run --example extract_shader

use era::EraArchive;
use std::fs::File;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let era_path = std::env::args().nth(1).unwrap_or_else(|| {
        "/Users/dev/Documents/steamcmd/halo_wars/root.era".to_string()
    });
    
    println!("Opening ERA: {}", era_path);
    let mut archive = EraArchive::open(&era_path)?;
    
    // List all files containing "terrain" or "shader" or ".bin"
    println!("\nSearching for terrain/shader/bin files...\n");
    let mut found = Vec::new();
    
    for (i, entry) in archive.iter().enumerate() {
        if let Some(name) = &entry.filename {
            let lower = name.to_lowercase();
            if lower.contains("terrain") || lower.contains("shader") || lower.ends_with(".bin") {
                println!("[{}] {}", i, name);
                found.push((i, name.clone()));
            }
        }
    }
    
    if found.is_empty() {
        println!("No matching files found.");
    } else {
        println!("\nFound {} matching files", found.len());
    }
    
    // Extract specific files
    for (idx, name) in &found {
        if name.to_lowercase().contains("gputerrain") || name.to_lowercase().contains("terrain") && name.ends_with(".bin") {
            println!("\nExtracting: {}", name);
            let data = archive.read_entry(*idx)?;
            let output_name = name.replace("\\", "_").replace("/", "_");
            let mut file = File::create(&output_name)?;
            file.write_all(&data)?;
            println!("  Written {} bytes to {}", data.len(), output_name);
        }
    }
    
    Ok(())
}

