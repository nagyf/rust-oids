mod mainloop;
mod ev;
use core::util::Cycle;
use core::math;
use core::math::Directional;
use core::math::Smooth;

use backend::obj;
use backend::world;
use backend::systems;

use backend::systems::System;

use frontend::input;
use frontend::input::*;
use frontend::render;

use std::time::{SystemTime, Duration, SystemTimeError};
use cgmath;
use cgmath::Matrix4;
use backend::obj::*;
use core::geometry::*;

pub fn run() {
	mainloop::main_loop();
}

pub struct Viewport {
	width: u32,
	height: u32,
	pub ratio: f32,
	pub scale: f32,
}

impl Viewport {
	fn rect(w: u32, h: u32, scale: f32) -> Viewport {
		Viewport {
			width: w,
			height: h,
			ratio: (w as f32 / h as f32),
			scale: scale,
		}
	}

	fn to_world(&self, x: f32, y: f32) -> cgmath::Vector2<f32> {
		let dx = self.width as f32 / self.scale;
		let tx = (x - (self.width as f32 * 0.5)) / dx;
		let ty = ((self.height as f32 * 0.5) - y) / dx;
		cgmath::Vector2::new(tx, ty)
	}
}

pub struct App {
	pub viewport: Viewport,
	input_state: input::InputState,
	wall_clock_start: SystemTime,
	frame_count: u32,
	frame_start: SystemTime,
	frame_elapsed: f32,
	frame_smooth: math::MovingAverage<f32>,
	is_running: bool,
	//
	light_position: Position,
	camera: math::Inertial<f32>,
	lights: Cycle<[f32; 4]>,
	backgrounds: Cycle<[f32; 4]>,
	//
	world: world::World,

	physics: systems::PhysicsSystem,
	animation: systems::AnimationSystem,
	game: systems::GameSystem,
	ai: systems::AiSystem,
}

pub struct Environment {
	pub light: [f32; 4],
	pub light_position: Position,
	pub background: [f32; 4],
}

pub struct Update {
	pub frame_count: u32,
	pub wall_clock_elapsed: Duration,
	pub frame_elapsed: f32,
	pub frame_time: f32,
	pub frame_time_smooth: f32,
	pub fps: f32,
}

impl App {
	pub fn new(w: u32, h: u32, scale: f32) -> App {
		App {
			viewport: Viewport::rect(w, h, scale),
			input_state: input::InputState::default(),

			// testbed, will need a display/render subsystem
			light_position: Position::new(10.0, 10.0),
			camera: Self::init_camera(),
			lights: Self::init_lights(),
			backgrounds: Self::init_backgrounds(),

			world: world::World::new(),
			// subsystem, need to update each
			physics: systems::PhysicsSystem::new(),
			animation: systems::AnimationSystem::new(),
			game: systems::GameSystem::new(),
			ai: systems::AiSystem::new(),

			// runtime and timing
			frame_count: 0u32,
			frame_elapsed: 0.0f32,
			frame_start: SystemTime::now(),
			wall_clock_start: SystemTime::now(),
			frame_smooth: math::MovingAverage::new(120),
			is_running: true,
		}
	}

	fn init_camera() -> math::Inertial<f32> {
		math::Inertial::new(10.0, 1. / 180., 0.5)
	}

	fn init_lights() -> Cycle<[f32; 4]> {
		Cycle::new(&[[1.0, 1.0, 1.0, 1.0],
		             [3.1, 3.1, 3.1, 1.0],
		             [10.0, 10.0, 10.0, 1.0],
		             [31.0, 31.0, 31.0, 1.0],
		             [100.0, 100.0, 100.0, 1.0],
		             [0.001, 0.001, 0.001, 1.0],
		             [0.01, 0.01, 0.01, 1.0],
		             [0.1, 0.1, 0.1, 1.0],
		             [0.31, 0.31, 0.31, 0.5]])
	}

	fn init_backgrounds() -> Cycle<[f32; 4]> {
		Cycle::new(&[[0.05, 0.07, 0.1, 1.0],
		             [0.5, 0.5, 0.5, 0.5],
		             [1.0, 1.0, 1.0, 1.0],
		             [3.1, 3.1, 3.1, 1.0],
		             [10.0, 10.0, 10.0, 1.0],
		             [0., 0., 0., 1.0],
		             [0.01, 0.01, 0.01, 1.0]])
	}

	fn new_resource(&mut self, pos: Position) {
		let id = self.world.new_resource(pos);
		self.register(id);
	}

	fn new_minion(&mut self, pos: Position) {
		let id = self.world.new_minion(pos);
		self.register(id);
	}

	fn register(&mut self, id: obj::Id) {
		let found = self.world.friend_mut(id);
		self.physics.register(found.unwrap());
	}

	pub fn on_app_event(&mut self, e: ev::Event) {
		match e {
			ev::Event::CamUp => self.camera.push(math::Direction::Up),
			ev::Event::CamDown => self.camera.push(math::Direction::Down),
			ev::Event::CamLeft => self.camera.push(math::Direction::Left),
			ev::Event::CamRight => self.camera.push(math::Direction::Right),

			ev::Event::CamReset => {
				self.camera.reset();
			}
			ev::Event::NextLight => {
				self.lights.next();
			}
			ev::Event::PrevLight => {
				self.lights.prev();
			}
			ev::Event::NextBackground => {
				self.backgrounds.next();
			}
			ev::Event::PrevBackground => {
				self.backgrounds.prev();
			}

			ev::Event::Reload => {}

			ev::Event::AppQuit => self.quit(),

			ev::Event::MoveLight(pos) => self.light_position = pos,
			ev::Event::NewMinion(pos) => self.new_minion(pos),
			ev::Event::NewResource(pos) => self.new_resource(pos),
			_ => {}
		}
	}

	pub fn quit(&mut self) {
		self.is_running = false;
	}

	pub fn is_running(&self) -> bool {
		self.is_running
	}

	pub fn on_input_event(&mut self, e: &input::Event) {
		self.input_state.event(e);
	}

	fn update_input(&mut self, _: f32) {
		let mut events = Vec::new();

		macro_rules! on_key_held {
			[$($key:ident -> $app_event:ident),*] => (
				$(if self.input_state.key_pressed(Key::$key) { events.push(ev::Event::$app_event); })
				*
			)
		}
		macro_rules! on_key_pressed_once {
			[$($key:ident -> $app_event:ident),*] => (
				$(if self.input_state.key_once(Key::$key) { events.push(ev::Event::$app_event); })
				*
			)
		}
		on_key_held! [
			Up -> CamUp,
			Down -> CamDown,
			Left -> CamLeft,
			Right-> CamRight
		];

		on_key_pressed_once! [
			F5 -> Reload,
			L -> NextLight,
			B -> NextBackground,
			K -> PrevLight,
			V -> PrevBackground,
			Esc -> AppQuit
		];

		let mouse_pos = self.input_state.mouse_position();
		let view_pos = self.to_view(mouse_pos.x, mouse_pos.y);
		let world_pos = self.to_world(self.input_state.mouse_position());

		if self.input_state.key_once(Key::MouseRight) {
			if self.input_state.any_ctrl_pressed() {
				events.push(ev::Event::NewMinion(world_pos));
			} else {
				events.push(ev::Event::NewResource(world_pos));
			}
		}

		if self.input_state.key_pressed(Key::MouseLeft) {
			events.push(ev::Event::MoveLight(world_pos));
		}

	}

	fn to_view<T>(&self, x: T, y: T) -> Position
		where T: Into<f32> {
		self.viewport.to_world(x.into(), y.into())
	}

	fn to_world(&self, t: Position) -> Position {
		t + self.camera.position()
	}

	pub fn on_resize(&mut self, width: u32, height: u32) {
		self.viewport = Viewport::rect(width, height, self.viewport.scale);
	}

	fn from_transform(transform: &Transform) -> Matrix4<f32> {
		use cgmath::Rotation3;
		let position = transform.position;
		let angle = transform.angle;
		let rot = Matrix4::from(cgmath::Quaternion::from_axis_angle(cgmath::Vector3::unit_z(), cgmath::rad(angle)));
		let trans = Matrix4::from_translation(cgmath::Vector3::new(position.x, position.y, 0.0));

		trans * rot
	}

	fn from_position(position: &Position) -> Matrix4<f32> {
		Matrix4::from_translation(cgmath::Vector3::new(position.x, position.y, 0.0))
	}

	fn render_minions(&self, renderer: &mut render::Draw) {
		for (_, b) in self.world.minions.agents() {
			for segment in b.segments() {
				let body_transform = Self::from_transform(&segment.transform());

				let mesh = &segment.mesh();
				let fixture_scale = Matrix4::from_scale(mesh.shape.radius());
				let transform = body_transform * fixture_scale;

				match mesh.shape {
					obj::Shape::Ball { .. } => {
						renderer.draw_ball(&transform, segment.color());
					}
					obj::Shape::Star { .. } => {
						renderer.draw_star(&transform, &mesh.vertices[..], segment.color());
					}
					obj::Shape::Box { ratio, .. } => {
						renderer.draw_quad(&transform, ratio, segment.color());
					}
					obj::Shape::Triangle { .. } => {
						renderer.draw_triangle(&transform, &mesh.vertices[0..3], segment.color());
					}
				}
			}
		}
	}

	fn render_extent(&self, renderer: &mut render::Draw) {}

	fn render_hud(&self, renderer: &mut render::Draw) {
		let transform = Self::from_position(&self.light_position);
		renderer.draw_ball(&transform, self.lights.get());
	}

	pub fn render(&self, renderer: &mut render::Draw) {
		self.render_minions(renderer);
		self.render_extent(renderer);
		self.render_hud(renderer);
	}

	pub fn environment(&self) -> Environment {
		Environment {
			light: self.lights.get(),
			light_position: self.light_position,
			background: self.backgrounds.get(),
		}
	}

	fn update_systems(&mut self, dt: f32) {
		self.animation.update_world(dt, &mut self.world);

		self.game.update_world(dt, &mut self.world);

		self.ai.follow_me(self.light_position);
		self.ai.update_world(dt, &mut self.world);

		self.physics.update_world(dt, &mut self.world);
	}

	fn init_systems(&mut self) {
		self.animation.init(&mut self.world);

		self.ai.init(&mut self.world);

		self.game.init(&mut self.world);

		self.physics.init(&mut self.world);
	}

	pub fn update(&mut self) -> Result<Update, SystemTimeError> {
		let dt = try!(self.frame_start.elapsed());
		let frame_time = (dt.as_secs() as f32) + (dt.subsec_nanos() as f32) * 1e-9;
		let frame_time_smooth = self.frame_smooth.smooth(frame_time);


		self.frame_elapsed += frame_time;
		self.frame_start = SystemTime::now();

		self.camera.update(frame_time_smooth);
		self.update_input(frame_time_smooth);
		self.update_systems(frame_time_smooth);
		self.frame_count += 1;

		Ok(Update {
			wall_clock_elapsed: self.wall_clock_start.elapsed().unwrap_or_else(|_| Duration::new(0, 0)),
			frame_count: self.frame_count,
			frame_elapsed: self.frame_elapsed,
			frame_time: frame_time,
			frame_time_smooth: frame_time_smooth,
			fps: 1.0 / frame_time_smooth,
		})
	}
}
