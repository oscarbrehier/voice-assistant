use std::process::Command;

pub fn spawn_app(app: String) -> anyhow::Result<()> {

	Command::new(app)
		.output()
		.expect("failed to spawn app");

	Ok(())

}