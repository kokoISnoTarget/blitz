use thunder::launch_static_html;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(EnvFilter::new("thunder"))
            .init();
    }
    launch_static_html(include_str!("./test.html"));
}
