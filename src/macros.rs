#[macro_export]
macro_rules! channel_handle_result {
    ($($x:tt)+) => {
        match ($($x)+) {
            Ok(val) => val,
            Err(err) => {
                use tracing::error;
                error!("{:?}", err);
                return;
            },
        }
    };
}
