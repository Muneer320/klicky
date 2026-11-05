pub(crate) fn key_playback_volume(volume: f32, key_name: &str) -> f32 {
    if !is_function_key(key_name) {
        return volume;
    }
    if volume <= 0.0 {
        0.0
    } else {
        (volume + 0.2).min(1.0)
    }
}

pub(crate) fn is_function_key(key_name: &str) -> bool {
    key_name.starts_with('F') && key_name.len() <= 3 && key_name[1..].parse::<u32>().is_ok()
}
