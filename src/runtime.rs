// SPDX-License-Identifier: MIT OR Apache-2.0

use {once_cell::sync::Lazy, tokio::runtime::Runtime};

// I/O-bound workload (ADB polling + network I/O): 4 workers is sufficient.
pub static TOKIO_RT: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("Failed to create global Tokio runtime")
});
