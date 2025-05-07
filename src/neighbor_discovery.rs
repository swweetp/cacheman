pub mod advertise;
pub mod browse;
mod zbus_binding;

const DESTINATION: &str = "org.freedesktop.Avahi";

#[cfg(not(test))]
const SERVICE_TYPE: &str = "_cacheman._tcp";
#[cfg(test)]
const SERVICE_TYPE: &str = "_test-cacheman._tcp";

#[cfg(test)]
mod test {
    use rand::{
        distr::{Alphabetic, SampleString},
        rng,
    };

    pub fn generate_random_hostname(prefix: &str) -> String {
        if prefix.len() > 54 {
            panic!("Prefix length exceeds 54 characters");
        }
        if prefix.is_empty() {
            Alphabetic.sample_string(&mut rng(), 60)
        } else {
            let suffix_length = 60 - 1 - prefix.len();
            let suffix = Alphabetic.sample_string(&mut rng(), suffix_length);
            format!("{}-{}", prefix, suffix)
        }
    }
}
