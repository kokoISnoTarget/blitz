use std::{
    env::{args, current_dir},
    str::FromStr,
};

use thunder::launch_static_html;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

enum Examples {
    Dynamic,
    Event,
    Iter,
}
impl FromStr for Examples {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "dynamic" => Examples::Dynamic,
            "event" => Examples::Event,
            "iter" => Examples::Iter,
            _ => return Err(()),
        })
    }
}
impl Examples {
    fn get_html(&self) -> &'static str {
        match self {
            Examples::Dynamic => include_str!("./dynamic.html"),
            Examples::Event => include_str!("./event.html"),
            Examples::Iter => include_str!("./iter.html"),
        }
    }
}
fn main() {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(EnvFilter::new("thunder"))
            .init();
    }
    let html = args()
        .nth(1)
        .map(|arg| arg.parse::<Examples>().unwrap_or(Examples::Iter).get_html())
        .unwrap();
    let path = format!("file://{}", current_dir().unwrap().display());
    launch_static_html(&path, html);
}
