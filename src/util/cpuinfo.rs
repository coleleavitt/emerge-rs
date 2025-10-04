// cpuinfo.rs -- CPU information utilities

use regex::Regex;

lazy_static::lazy_static! {
    static ref JOBS_REGEX: Regex = Regex::new(r".*(j|--jobs=\s)\s*([0-9]+)").unwrap();
}

pub fn get_cpu_count() -> Option<usize> {
    std::thread::available_parallelism().map(|n| n.get()).ok()
}

pub fn makeopts_to_job_count(makeopts: &str) -> Option<usize> {
    if makeopts.is_empty() {
        return get_cpu_count();
    }

    if let Some(captures) = JOBS_REGEX.captures(makeopts) {
        if let Some(job_str) = captures.get(2) {
            if let Ok(jobs) = job_str.as_str().parse::<usize>() {
                return Some(jobs);
            }
        }
    }

    get_cpu_count()
}