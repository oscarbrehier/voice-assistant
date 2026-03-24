use chrono::Local;

pub fn get_time() -> anyhow::Result<String> {

	let local_time = Local::now();
	let time_str = local_time.format("%H:%M").to_string();

	Ok(format!("It is {time_str}"))

}