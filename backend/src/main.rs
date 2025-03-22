use rfd::FileDialog;
use routes::music::save_music::load_folder_music;
use std::path::Path;
use std::{fs, io::Write};

mod config;
mod core;
mod lobic_db;
mod mail;
mod routes;
mod schema;
mod utils;
use std::fs::File;

use config::{server_ip, COVER_IMG_STORAGE, MUSIC_STORAGE, PLAYLIST_COVER_IMG_STORAGE, PORT, USER_PFP_STORAGE};
use core::{app_state::AppState, migrations::run_migrations};
use dotenv::dotenv;

//USAGE:
//		cargo run -> start the server
//		cargo run load -> load musci and start the server

#[tokio::main]
async fn main() {
	dotenv().ok();
	create_storage_directories().expect("Failed to create storage directories");
	write_ip_to_frontend(&server_ip(), &PORT).expect("Failed to load the ip to frontend ");

	let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env file");
	run_migrations(&db_url);

	let app_state = AppState::new();
	let app = core::routes::configure_routes(app_state.clone())
		.layer(axum::middleware::from_fn(core::server::logger))
		.layer(core::server::configure_cors());

	let args: Vec<String> = std::env::args().collect();
	if args.len() > 1 && args[1] == "load" {
		load_music(app_state);
	}

	tracing_subscriber::fmt().pretty().init();
	core::server::start_server(app, &server_ip(), &PORT).await;
}

fn create_storage_directories() -> std::io::Result<()> {
	// Create the base storage directory if it doesn't exist
	if !Path::new("storage/").exists() {
		fs::create_dir("storage/")?;
	}

	// Create subdirectories
	let subdirectories = [
		COVER_IMG_STORAGE,
		MUSIC_STORAGE,
		USER_PFP_STORAGE,
		PLAYLIST_COVER_IMG_STORAGE,
	];

	for dir in subdirectories {
		if !Path::new(dir).exists() {
			fs::create_dir_all(dir)?;
		}
	}
	Ok(())
}

fn write_ip_to_frontend(ip: &str, port: &str) -> std::io::Result<()> {
	let const_ts_path = String::from("../frontend/src/const.ts");
	let mut file = File::create(const_ts_path)?;
	let const_ts =
		format!("export const SERVER_IP = 'http://{ip}:{port}'; \nexport const WS_SERVER_IP = 'ws://{ip}:{port}/ws'; ");
	file.write_all(const_ts.as_bytes())?;
	Ok(())
}

fn load_music(app_state: AppState) {
	println!("Loading music files...");
	// Open a folder selection dialog box
	//might not be cross platform
	let folder = FileDialog::new()
		.set_title("Select a folder containing music files")
		.pick_folder();

	// Check if the user selected a folder
	if let Some(folder_path) = folder {
		let folder_path = folder_path.display().to_string();
		println!("Selected folder: {folder_path}");
		load_folder_music(app_state, folder_path);
	} else {
		println!("No folder selected.");
	}
}
