//! TOML file read/write helpers.
//!
//! Provides two universal functions that work with explicit paths,
//! and two convenience wrappers that work relative to the current
//! working directory.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use toml::{from_str, to_string_pretty};

/// Serializes a structure and writes it to a TOML file at the specified full path.
///
/// # Parameters
/// - `struct_to_write`: Structure to serialize.
/// - `file_path`: Full target file path.
///
/// # Returns
/// - `Ok(())` if the file was written successfully.
/// - `Err(String)` if serialization or file writing failed.
pub fn write_toml_full_path(
    struct_to_write: &impl Serialize,
    file_path: impl AsRef<Path>,
) -> Result<(), String> {
    let file_path = file_path.as_ref();

    // Serialize structure to TOML string
    let toml_text = to_string_pretty(struct_to_write)
        .map_err(|err| format!(
            "TOML serialization failed for {}: {}",
            file_path.display(),
            err
        ))?;

    // Write string to file
    fs::write(file_path, toml_text)
        .map_err(|err| format!(
            "File write failed for {}: {}",
            file_path.display(),
            err
        ))?;

    Ok(())
}   // write_toml_full_path()

/// Reads a TOML file from the specified full path and deserializes it into a new structure.
///
/// # Parameters
/// - `file_path`: Full source file path.
///
/// # Returns
/// - `Ok(T)` with the deserialized structure.
/// - `Err(String)` if file reading or deserialization failed.
pub fn read_toml_full_path<T>(file_path: impl AsRef<Path>) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let file_path = file_path.as_ref();

    // Read file contents into a string
    let file_content = fs::read_to_string(file_path)
        .map_err(|err| format!(
            "File read failed for {}: {}",
            file_path.display(),
            err
        ))?;

    // Deserialize the TOML string into the requested structure
    let deserialized_struct = from_str(&file_content)
        .map_err(|err| format!(
            "TOML deserialization failed for {}: {}",
            file_path.display(),
            err
        ))?;

    Ok(deserialized_struct)
}   // read_toml_full_path()

/// Serializes a structure and writes it to a TOML file in the current working directory.
///
/// # Parameters
/// - `struct_to_write`: Structure to serialize.
/// - `file_name`: File name relative to the current working directory.
///
/// # Returns
/// - `Ok(())` if the file was written successfully.
/// - `Err(String)` if path resolution, serialization, or file writing failed.
pub fn write_toml_file(
    struct_to_write: &impl Serialize,
    file_name: &str,
) -> Result<(), String> {
    let file_path = _build_path_in_current_dir(file_name)?;

    write_toml_full_path(struct_to_write, &file_path)
}   // write_toml_file()

/// Reads a TOML file from the current working directory and deserializes it into a new structure.
///
/// # Parameters
/// - `file_name`: File name relative to the current working directory.
///
/// # Returns
/// - `Ok(T)` with the deserialized structure.
/// - `Err(String)` if path resolution, file reading, or deserialization failed.
pub fn read_toml_file<T>(file_name: &str) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let file_path = _build_path_in_current_dir(file_name)?;

    read_toml_full_path(&file_path)
}   // read_toml_file()

/**************************************** Private *************************************************/

/// Builds a complete path by appending the given file name to the current working directory.
///
/// # Application
/// Used internally by convenience functions to resolve relative file names into
/// absolute paths before reading or writing.
///
/// # Parameters
/// - `file_name`: Name of the file.
///
/// # Returns
/// - `Ok(PathBuf)` containing the combined absolute path.
/// - `Err(String)` if the current working directory cannot be determined.
fn _build_path_in_current_dir(file_name: &str) -> Result<PathBuf, String> {
    let current_directory = std::env::current_dir()
        .map_err(|err| format!("Failed to get current working directory: {}", err))?;

    Ok(current_directory.join(file_name))
}   // _build_path_in_current_dir()