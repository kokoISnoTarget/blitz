use thunder::launch_url;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(EnvFilter::new("thunder"))
            .init();
        tracing::info!("Tracing initialized");
    }

    //launch_static_html(include_str!("../../../google.html"));
    launch_url("http://127.0.0.1:7001");
}
