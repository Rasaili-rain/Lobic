use crate::config::{COVER_IMG_STORAGE, MUSIC_STORAGE};
use crate::core::app_state::AppState;
use crate::lobic_db::models::Music;
use crate::schema::music::dsl::*;

use diesel::prelude::*;
use id3::{frame::PictureType, Tag, TagLike};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

pub fn load_folder_music(app_state: AppState, folder_path: String) {
	let mut db_conn = match app_state.db_pool.get() {
		Ok(conn) => conn,
		Err(_) => return,
	};

	// Convert Windows path to WSL path if needed
	let path = normalize_path(&folder_path);
	let path = Path::new(&path);

	let mut saved_count = 0;

	if path.is_dir() {
		for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
			if is_music_file(entry.path()) {
				match process_music_file(entry.path(), &mut db_conn) {
					Ok(_) => saved_count += 1,
					Err(_) => (),
				}
			}
		}
	} else if is_music_file(path) {
		match process_music_file(path, &mut db_conn) {
			Ok(_) => saved_count += 1,
			Err(_) => (),
		}
	}
	println!("processed {saved_count} files");
}

fn normalize_path(path: &str) -> String {
	if cfg!(windows) {
		// On Windows, convert forward slashes to backslashes
		path.replace('/', "\\")
	} else {
		// If running under WSL, convert Windows paths to WSL paths
		if path.contains('\\') || path.contains(':') {
			convert_windows_to_wsl_path(path)
		} else {
			path.to_string()
		}
	}
}

fn convert_windows_to_wsl_path(windows_path: &str) -> String {
	let path = windows_path.replace('\\', "/");

	// Check if it's a Windows-style path (e.g., C:\...)
	if path.chars().nth(1) == Some(':') {
		let drive_letter = path.chars().next().unwrap().to_lowercase().to_string();
		format!("/mnt/{}/{}", drive_letter, &path[3..])
	} else {
		path
	}
}

fn is_music_file(path: &Path) -> bool {
	match path.extension() {
		Some(ext) => matches!(
			ext.to_str().unwrap_or("").to_lowercase().as_str(),
			"mp3" | "m4a" | "flac" | "wav" | "ogg"
		),
		None => false,
	}
}

fn process_music_file(path: &Path, db_conn: &mut SqliteConnection) -> Result<(), Box<dyn std::error::Error>> {
	let path_str = path.to_str().ok_or("Invalid path")?;

	// Read ID3 tags
	let tag = Tag::read_from_path(path_str).unwrap_or_else(|_| Tag::new());

	let curr_artist = tag.artist().unwrap_or("Unknown Artist");
	let curr_title = tag.title().unwrap_or("Unknown Title");
	let curr_album = tag.album().unwrap_or("Unknown Album");

	let curr_music_id = generate_uuid_from_metadata(curr_artist, curr_title, curr_album);

	// Create the music_db directory if it doesn't exist
	let music_db_dir = PathBuf::from(MUSIC_STORAGE);
	fs::create_dir_all(&music_db_dir)?;

	let new_file_path = music_db_dir.join(format!("{}.mp3", curr_music_id));

	// Copy the music file to the new location
	fs::copy(path, &new_file_path)?;

	let file = fs::File::open(path_str)?;
	let duration_u64 = mp3_duration::from_file(&file)?.as_secs();
	let curr_duration = i64::try_from(duration_u64)?; // Convert u64 to i64, will error if too large

	let curr_music = Music {
		music_id: curr_music_id.to_string(),
		artist: curr_artist.to_string(),
		title: curr_title.to_string(),
		album: curr_album.to_string(),
		genre: tag.genre().unwrap_or("Unknown Genre").to_string(),
		times_played: 0,
		duration: curr_duration,
	};

	extract_cover_art(path_str, &curr_artist, &curr_album)?;

	diesel::insert_into(music).values(&curr_music).execute(db_conn)?;

	Ok(())
}

fn extract_cover_art(mp3_path: &str, curr_artist: &str, curr_album: &str) -> Result<(), Box<dyn std::error::Error>> {
	let tag = Tag::read_from_path(mp3_path)?;
	let pictures: Vec<_> = tag.pictures().collect();

	if let Some(picture) = pictures.iter().find(|pic| pic.picture_type == PictureType::CoverFront) {
		// Create platform-independent path for cover_images directory

		//create a uuid from artist and album name to store image
		let mut hasher = DefaultHasher::new();
		curr_artist.hash(&mut hasher);
		curr_album.hash(&mut hasher);
		let hash = hasher.finish();
		let img_uuid = Uuid::from_u64_pair(hash, hash);

		let cover_dir = PathBuf::from(COVER_IMG_STORAGE);
		fs::create_dir_all(&cover_dir)?;

		let output_path = cover_dir.join(format!("{}.png", img_uuid));
		let mut file = fs::File::create(&output_path)?;
		file.write_all(&picture.data)?;

		Ok(())
	} else {
		Err("No cover art found in the MP3 file".into())
	}
}

//assumes all mp3 have unique sets of metadata
fn generate_uuid_from_metadata(curr_artist: &str, curr_title: &str, curr_album: &str) -> Uuid {
	let mut hasher = DefaultHasher::new();
	curr_artist.hash(&mut hasher);
	curr_title.hash(&mut hasher);
	curr_album.hash(&mut hasher);
	let hash = hasher.finish();

	// Convert the hash to a UUID
	Uuid::from_u64_pair(hash, hash)
}
