use chrono::Local;

pub fn get_time(template: Option<String>) -> anyhow::Result<String> {

	let local_time = Local::now();
	let time_str = local_time.format("%H:%M").to_string();

	if let Some(template) = template {
		let message = template.replace("{{time}}", &time_str);
		return Ok(message);
	}

	Ok(format!("It is {time_str}"))

}