// controller.rs — Task lifecycle, concurrency, network, cookie extraction
//
// Houses: TaskController, TaskControllerStore, semaphore (DOWNLOAD_SEMAPHORE),
// MAX_CONCURRENT, NETWORK_PROXY, NETWORK_SPEED_LIMIT,
// pause/resume/cancel logic, extract_browser_cookies, extract_cookies_via_rookie.
//
// Currently these live in mod.rs; new controller code should be added here.
