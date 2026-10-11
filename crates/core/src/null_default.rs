use serde::{Deserialize, Deserializer};

pub fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

pub fn or_true<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(deserializer)?.unwrap_or(true))
}

pub fn or_one_u8<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    Ok(Option::<u8>::deserialize(deserializer)?.unwrap_or(1))
}

pub fn or_one_u32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    Ok(Option::<u32>::deserialize(deserializer)?.unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Sample {
        #[serde(default, deserialize_with = "or_default")]
        name: String,
        #[serde(default = "yes", deserialize_with = "or_true")]
        flag: bool,
        #[serde(default = "one", deserialize_with = "or_one_u32")]
        count: u32,
        #[serde(default = "small_one", deserialize_with = "or_one_u8")]
        small: u8,
    }

    const fn yes() -> bool {
        true
    }

    const fn one() -> u32 {
        1
    }

    const fn small_one() -> u8 {
        1
    }

    fn sample(json: &str) -> Sample {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn null_means_the_same_as_leaving_the_field_out() {
        let defaults = Sample { name: String::new(), flag: true, count: 1, small: 1 };
        assert_eq!(sample("{}"), defaults);
        assert_eq!(sample(r#"{"name":null,"flag":null,"count":null,"small":null}"#), defaults);
        let given = sample(r#"{"name":"x","flag":false,"count":5,"small":3}"#);
        assert_eq!(given, Sample { name: "x".to_string(), flag: false, count: 5, small: 3 });
        assert!(serde_json::from_str::<Sample>(r#"{"count":"two"}"#).is_err());
    }
}
