use thunder::launch_url;

fn main() {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::fmt::init();
        tracing::info!("Tracing initialized");
    }

    //launch_static_html(include_str!("../../../google.html"));
    launch_url("https://google.com");
}
