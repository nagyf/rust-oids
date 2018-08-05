use app::constants::*;
use chrono::DateTime;
use chrono::Utc;
use gl;
use glutin;
use glutin::GlContext;
use image;
use image::ImageBuffer;
use rayon;
use std::fs::create_dir_all;
use std::path::PathBuf;

pub struct Capture {
	seq: usize,
	capture_path: PathBuf,
	capture_prefix: String,
	enabled: bool,
	w: u32,
	h: u32,
	images: Vec<ImageBuffer<image::Rgb<u8>, Vec<u8>>>,
}

impl Capture {
	// Initializes capture system
	pub fn init(window: &glutin::GlWindow) -> Capture {
		//use gl;
		gl::ReadPixels::load_with(|s| window.get_proc_address(s) as *const _);
		let (w, h) = window.get_inner_size().unwrap();
		let now: DateTime<Utc> = Utc::now();
		Capture {
			seq: 0,
			capture_path: PathBuf::from(CAPTURE_FOLDER).join(now.format(CAPTURE_FOLDER_TIMESTAMP_PATTERN).to_string()),
			capture_prefix: String::from(CAPTURE_FILENAME_PREFIX),
			enabled: false,
			w,
			h,
			images: Vec::new(),
		}
	}

	// Capture current framebuffer if recording is enabled
	pub fn screen_grab(&mut self) {
		if self.enabled {
			let mut buf: Vec<u8> = vec![0u8; self.w as usize * self.h as usize * 3];
			unsafe {
				gl::ReadPixels(
					0,
					0,
					self.w as i32,
					self.h as i32,
					gl::RGB,
					gl::UNSIGNED_BYTE,
					buf.as_mut_ptr() as *mut _,
				);
			}
			self.seq += 1;
			let filename = self.capture_prefix.clone() + &format!("{:08}.png", self.seq);
			let full_path = self.capture_path.join(filename);
			let w = self.w;
			let h = self.h;
			rayon::spawn(move || {
				let mut img = ImageBuffer::new(w, h);
				for i in 0..h {
					for j in 0..w {
						let base: usize = 3 * (j + (h - i - 1) * w) as usize;
						let r = buf[base + 0];
						let g = buf[base + 1];
						let b = buf[base + 2];
						img.put_pixel(j, i, image::Rgb([r, g, b]));
					}
				}
				println!("Saving image {}", full_path.to_str().unwrap());
				img.save(full_path).expect("Could not write image");
			});
		}
	}

	fn flush(&mut self) {
		match create_dir_all(self.capture_path.clone()) {
			Ok(_) => {
				for img in &self.images {
					self.seq += 1;
					let filename = self.capture_prefix.clone() + &format!("{:08}.png", self.seq);
					let full_path = self.capture_path.join(filename);
					println!("Saving image {}", full_path.to_str().unwrap());
					img.save(full_path).expect("Could not write image");
				}
			}
			Err(msg) => error!(
				"Could not create capture directory {}: {}",
				self.capture_path.to_str().unwrap(),
				msg
			),
		}
		self.images.clear()
	}

	// Remote control, detects state changes
	pub fn enable(&mut self, enabled: bool) {
		if enabled != self.enabled {
			self.toggle()
		}
	}

	// Starts/restarts recording
	pub fn start(&mut self) { self.enabled = true }

	// Stops recording and flushes
	pub fn stop(&mut self) {
		if self.enabled {
			self.flush();
		}
		self.enabled = false;
	}

	pub fn enabled(&self) -> bool { self.enabled }

	pub fn toggle(&mut self) {
		if self.enabled {
			self.stop();
		} else {
			self.start();
		}
	}
}
