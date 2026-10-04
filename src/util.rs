use std::fmt::Display;

pub trait ResultEx {
    #[must_use]
    fn log_info(self) -> Self;

    #[must_use]
    fn log_warn(self) -> Self;

    #[must_use]
    fn log_error(self) -> Self;
}

impl<T, Err: Display> ResultEx for Result<T, Err> {
    fn log_info(self) -> Self {
        self.map_err(|e| {
            log::info!("{e}");
            e
        })
    }

    fn log_warn(self) -> Self {
        self.map_err(|e| {
            log::warn!("{e}");
            e
        })
    }

    fn log_error(self) -> Self {
        self.map_err(|e| {
            log::error!("{e}");
            e
        })
    }
}
