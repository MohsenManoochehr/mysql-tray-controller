#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_os = "windows"))]
compile_error!("mysql-tray-controller only supports Windows.");

use anyhow::{anyhow, Context, Result};
use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::Write,
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};
use windows_service::{
    service::{ServiceAccess, ServiceState},
    service_manager::{ServiceManager, ServiceManagerAccess},
};
use windows_sys::Win32::{
    Foundation::ERROR_SERVICE_DOES_NOT_EXIST,
    UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_YESNO, SW_HIDE,
        },
    },
};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};
use winreg::{enums::*, RegKey};

const APP_NAME: &str = "MySQL Tray Controller";

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const APP_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_STAR_MESSAGE: &str = "Enjoying the app? Please consider starring it on GitHub.";

const RUN_VALUE_NAME: &str = "MySQLTrayController";
const RUN_REGISTRY_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn main() {
    if let Err(error) = real_main() {
        write_error_log(&format!("{error:#}"));
        show_error(&format!("{error:#}"));
    }
}

fn real_main() -> Result<()> {
    let args: Vec<OsString> = env::args_os().collect();

    if let Some(action) = argument_value(&args, "--elevated-action") {
        let service_name = argument_value(&args, "--service")
            .ok_or_else(|| anyhow!("Missing --service argument"))?;

        let action = action.to_string_lossy();
        let service_name = service_name.to_string_lossy();

        if let Err(error) = perform_service_action(&service_name, &action) {
            write_error_log(&format!(
                "Service action failed: action={action}, service={service_name}\n{error:#}"
            ));
            show_error(&format!(
                "Could not {action} the Windows service \"{service_name}\".\n\n{error:#}"
            ));
        }

        return Ok(());
    }

    let (config, config_path) = Config::load_or_create()?;

    let event_loop = EventLoop::new().context("Could not create the Windows event loop")?;
    let mut app = App::new(config, config_path);
    event_loop
        .run_app(&mut app)
        .context("The tray event loop stopped unexpectedly")?;

    Ok(())
}

fn argument_value(args: &[OsString], key: &str) -> Option<OsString> {
    args.iter()
        .position(|value| value == OsStr::new(key))
        .and_then(|index| args.get(index + 1))
        .cloned()
}

#[derive(Clone, Debug)]
struct Config {
    service_name: String,
    refresh_interval: Duration,
}

impl Config {
    fn load_or_create() -> Result<(Self, PathBuf)> {
        let path = config_path()?;

        if !path.exists() {
            let config = Self {
                service_name: detect_mysql_service(),
                refresh_interval: Duration::from_secs(2),
            };
            config.save(&path)?;
            return Ok((config, path));
        }

        let config = Self::load(&path)?;
        Ok((config, path))
    }

    fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Could not read {}", path.display()))?;

        let mut service_name: Option<String> = None;
        let mut refresh_seconds = 2_u64;

        for raw_line in content.lines() {
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let key = key.trim();
            let value = value.trim().trim_matches('"');

            match key {
                "service_name" if !value.is_empty() => {
                    service_name = Some(value.to_owned());
                }
                "refresh_interval_seconds" => {
                    if let Ok(seconds) = value.parse::<u64>() {
                        refresh_seconds = seconds.clamp(1, 60);
                    }
                }
                _ => {}
            }
        }

        let service_name = service_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(detect_mysql_service);

        Ok(Self {
            service_name,
            refresh_interval: Duration::from_secs(refresh_seconds),
        })
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Could not create {}", parent.display()))?;
        }

        let content = format!(
            "# MySQL Tray Controller configuration\n\
             # Use the Windows SERVICE NAME, not its display name.\n\
             # Find it in services.msc -> MySQL service -> Properties.\n\
             service_name={}\n\
             refresh_interval_seconds={}\n",
            self.service_name,
            self.refresh_interval.as_secs()
        );

        fs::write(path, content).with_context(|| format!("Could not write {}", path.display()))
    }
}

fn app_data_dir() -> Result<PathBuf> {
    let app_data = env::var_os("APPDATA")
        .ok_or_else(|| anyhow!("The APPDATA environment variable is unavailable"))?;

    Ok(PathBuf::from(app_data).join("MySQLTrayController"))
}

fn config_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("config.ini"))
}

fn error_log_path() -> Option<PathBuf> {
    app_data_dir().ok().map(|path| path.join("error.log"))
}

fn write_error_log(message: &str) {
    let Some(path) = error_log_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}\n");
    }
}

fn detect_mysql_service() -> String {
    const CANDIDATES: &[&str] = &[
        "MySQL84",
        "MySQL80",
        "MySQL",
        "MariaDB",
        "mariadb",
        "wampmysqld64",
        "wampmariadb64",
    ];

    for candidate in CANDIDATES {
        if service_exists(candidate) {
            return (*candidate).to_owned();
        }
    }

    "MySQL84".to_owned()
}

fn service_exists(service_name: &str) -> bool {
    let Ok(manager) = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
    else {
        return false;
    };

    manager
        .open_service(service_name, ServiceAccess::QUERY_STATUS)
        .is_ok()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MySqlState {
    Running,
    Stopped,
    Starting,
    Stopping,
    Paused,
    Pending,
    NotFound,
    Error(String),
}

impl MySqlState {
    fn label(&self) -> &str {
        match self {
            Self::Running => "Running",
            Self::Stopped => "Stopped",
            Self::Starting => "Starting",
            Self::Stopping => "Stopping",
            Self::Paused => "Paused",
            Self::Pending => "Changing state",
            Self::NotFound => "Service not found",
            Self::Error(_) => "Status unavailable",
        }
    }

    fn color(&self) -> [u8; 4] {
        match self {
            Self::Running => [46, 204, 113, 255],
            Self::Stopped => [231, 76, 60, 255],
            Self::Starting | Self::Stopping | Self::Pending => [241, 196, 15, 255],
            Self::Paused => [52, 152, 219, 255],
            Self::NotFound | Self::Error(_) => [127, 140, 141, 255],
        }
    }

    fn is_running_choice(&self) -> bool {
        matches!(self, Self::Running | Self::Starting)
    }

    fn is_stopped_choice(&self) -> bool {
        matches!(self, Self::Stopped | Self::Stopping)
    }
}

fn query_service_state(service_name: &str) -> MySqlState {
    let manager = match ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
    {
        Ok(manager) => manager,
        Err(error) => return MySqlState::Error(error.to_string()),
    };

    let service = match manager.open_service(service_name, ServiceAccess::QUERY_STATUS) {
        Ok(service) => service,
        Err(windows_service::Error::Winapi(error))
            if error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) =>
        {
            return MySqlState::NotFound;
        }
        Err(error) => return MySqlState::Error(error.to_string()),
    };

    match service.query_status() {
        Ok(status) => match status.current_state {
            ServiceState::Running => MySqlState::Running,
            ServiceState::Stopped => MySqlState::Stopped,
            ServiceState::StartPending => MySqlState::Starting,
            ServiceState::StopPending => MySqlState::Stopping,
            ServiceState::Paused => MySqlState::Paused,
            ServiceState::ContinuePending | ServiceState::PausePending => MySqlState::Pending,
        },
        Err(error) => MySqlState::Error(error.to_string()),
    }
}

fn perform_service_action(service_name: &str, action: &str) -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("Could not connect to the Windows Service Control Manager")?;

    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::START | ServiceAccess::STOP;
    let service = manager
        .open_service(service_name, access)
        .with_context(|| format!("Could not open service \"{service_name}\""))?;

    match action {
        "start" => {
            let state = service.query_status()?.current_state;
            if !matches!(state, ServiceState::Running | ServiceState::StartPending) {
                let no_arguments: [&OsStr; 0] = [];
                service.start(&no_arguments)?;
                wait_for_state(&service, ServiceState::Running, Duration::from_secs(25))?;
            }
        }
        "stop" => {
            let state = service.query_status()?.current_state;
            if !matches!(state, ServiceState::Stopped | ServiceState::StopPending) {
                service.stop()?;
                wait_for_state(&service, ServiceState::Stopped, Duration::from_secs(25))?;
            }
        }
        "restart" => {
            let state = service.query_status()?.current_state;
            if !matches!(state, ServiceState::Stopped | ServiceState::StopPending) {
                service.stop()?;
                wait_for_state(&service, ServiceState::Stopped, Duration::from_secs(25))?;
            }

            let no_arguments: [&OsStr; 0] = [];
            service.start(&no_arguments)?;
            wait_for_state(&service, ServiceState::Running, Duration::from_secs(25))?;
        }
        other => return Err(anyhow!("Unsupported service action: {other}")),
    }

    Ok(())
}

fn wait_for_state(
    service: &windows_service::service::Service,
    expected: ServiceState,
    timeout: Duration,
) -> Result<()> {
    let started = Instant::now();

    while started.elapsed() < timeout {
        let status = service.query_status()?;
        if status.current_state == expected {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(250));
    }

    Err(anyhow!(
        "Timed out waiting for service state {:?}",
        expected
    ))
}

struct TrayUi {
    tray_icon: TrayIcon,
    status_item: MenuItem,
    running_item: CheckMenuItem,
    stopped_item: CheckMenuItem,
    restart_item: MenuItem,
    refresh_item: MenuItem,
    auto_start_item: CheckMenuItem,
    edit_config_item: MenuItem,
    reload_config_item: MenuItem,
    services_item: MenuItem,
    star_item: MenuItem,
    about_item: MenuItem,
    exit_item: MenuItem,
}

struct App {
    config: Config,
    config_path: PathBuf,
    ui: Option<TrayUi>,
    current_state: MySqlState,
    next_refresh: Instant,
}

impl App {
    fn new(config: Config, config_path: PathBuf) -> Self {
        Self {
            config,
            config_path,
            ui: None,
            current_state: MySqlState::Pending,
            next_refresh: Instant::now(),
        }
    }

    fn initialize_tray(&mut self) -> Result<()> {
        let status_item = MenuItem::new("Status: Checking...", false, None);
        let running_item = CheckMenuItem::new("Running", true, false, None);
        let stopped_item = CheckMenuItem::new("Stopped", true, false, None);
        let restart_item = MenuItem::new("Restart MySQL", true, None);
        let refresh_item = MenuItem::new("Refresh now", true, None);
        let auto_start_item = CheckMenuItem::new(
            "Start tray app with Windows",
            true,
            is_autostart_enabled(),
            None,
        );
        let edit_config_item = MenuItem::new("Edit configuration...", true, None);
        let reload_config_item = MenuItem::new("Reload configuration", true, None);
        let services_item = MenuItem::new("Open Windows Services", true, None);
        let star_item = MenuItem::new("★ Star this project on GitHub", true, None);
        let about_item = MenuItem::new("About MySQL Tray Controller", true, None);
        let exit_item = MenuItem::new("Exit", true, None);

        let separator_1 = PredefinedMenuItem::separator();
        let separator_2 = PredefinedMenuItem::separator();
        let separator_3 = PredefinedMenuItem::separator();

        let menu = Menu::new();
        menu.append_items(&[
            &status_item,
            &separator_1,
            &running_item,
            &stopped_item,
            &restart_item,
            &refresh_item,
            &separator_2,
            &auto_start_item,
            &edit_config_item,
            &reload_config_item,
            &services_item,
            &separator_3,
            &star_item,
            &about_item,
            &exit_item,
        ])
        .context("Could not build the tray menu")?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .with_menu_on_right_click(true)
            .with_tooltip(APP_NAME)
            .with_icon(make_status_icon(&MySqlState::Pending)?)
            .build()
            .context("Could not create the tray icon")?;

        self.ui = Some(TrayUi {
            tray_icon,
            status_item,
            running_item,
            stopped_item,
            restart_item,
            refresh_item,
            auto_start_item,
            edit_config_item,
            reload_config_item,
            services_item,
            star_item,
            about_item,
            exit_item,
        });

        Ok(())
    }

    fn refresh_status(&mut self) {
        let state = query_service_state(&self.config.service_name);
        self.apply_state(state);
        self.next_refresh = Instant::now() + self.config.refresh_interval;
    }

    fn apply_state(&mut self, state: MySqlState) {
        self.current_state = state.clone();

        let Some(ui) = &self.ui else {
            return;
        };

        let status_text = match &state {
            MySqlState::Error(error) => {
                format!("Status: {} ({})", state.label(), shorten(error, 45))
            }
            _ => format!("Status: {} ({})", state.label(), self.config.service_name),
        };

        ui.status_item.set_text(&status_text);
        ui.running_item.set_checked(state.is_running_choice());
        ui.stopped_item.set_checked(state.is_stopped_choice());

        let service_available = !matches!(state, MySqlState::NotFound | MySqlState::Error(_));
        ui.running_item.set_enabled(
            service_available && !matches!(state, MySqlState::Running | MySqlState::Starting),
        );
        ui.stopped_item.set_enabled(
            service_available && !matches!(state, MySqlState::Stopped | MySqlState::Stopping),
        );
        ui.restart_item
            .set_enabled(service_available && matches!(state, MySqlState::Running));

        let tooltip = format!(
            "{}: {} [{}]",
            APP_NAME,
            state.label(),
            self.config.service_name
        );

        let _ = ui.tray_icon.set_icon(Some(
            make_status_icon(&state).unwrap_or_else(|_| make_fallback_icon()),
        ));
        let _ = ui.tray_icon.set_tooltip(Some(tooltip));
    }

    fn process_menu_events(&mut self, event_loop: &ActiveEventLoop) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let Some(ui) = &self.ui else {
                continue;
            };

            if event.id() == ui.running_item.id() {
                if let Err(error) = launch_elevated_action("start", &self.config.service_name) {
                    write_error_log(&format!("{error:#}"));
                    show_error(&format!("{error:#}"));
                } else {
                    self.apply_state(MySqlState::Starting);
                    self.next_refresh = Instant::now() + Duration::from_millis(500);
                }
            } else if event.id() == ui.stopped_item.id() {
                if let Err(error) = launch_elevated_action("stop", &self.config.service_name) {
                    write_error_log(&format!("{error:#}"));
                    show_error(&format!("{error:#}"));
                } else {
                    self.apply_state(MySqlState::Stopping);
                    self.next_refresh = Instant::now() + Duration::from_millis(500);
                }
            } else if event.id() == ui.restart_item.id() {
                if let Err(error) = launch_elevated_action("restart", &self.config.service_name) {
                    write_error_log(&format!("{error:#}"));
                    show_error(&format!("{error:#}"));
                } else {
                    self.apply_state(MySqlState::Pending);
                    self.next_refresh = Instant::now() + Duration::from_millis(500);
                }
            } else if event.id() == ui.refresh_item.id() {
                self.next_refresh = Instant::now();
            } else if event.id() == ui.auto_start_item.id() {
                let enable = !is_autostart_enabled();

                match set_autostart(enable) {
                    Ok(()) => ui.auto_start_item.set_checked(enable),
                    Err(error) => {
                        ui.auto_start_item.set_checked(!enable);
                        write_error_log(&format!("{error:#}"));
                        show_error(&format!("{error:#}"));
                    }
                }
            } else if event.id() == ui.edit_config_item.id() {
                if let Err(error) = open_in_notepad(&self.config_path) {
                    write_error_log(&format!("{error:#}"));
                    show_error(&format!("{error:#}"));
                }
            } else if event.id() == ui.reload_config_item.id() {
                match Config::load(&self.config_path) {
                    Ok(config) => {
                        self.config = config;
                        self.next_refresh = Instant::now();
                    }
                    Err(error) => {
                        write_error_log(&format!("{error:#}"));
                        show_error(&format!("{error:#}"));
                    }
                }
            } else if event.id() == ui.services_item.id() {
                if let Err(error) = open_windows_services() {
                    write_error_log(&format!("{error:#}"));
                    show_error(&format!("{error:#}"));
                }
            } else if event.id() == ui.star_item.id() {
                if let Err(error) = open_url(APP_REPOSITORY) {
                    write_error_log(&format!("{error:#}"));
                    show_error(&format!("{error:#}"));
                }
            } else if event.id() == ui.about_item.id() {
                if show_about() {
                    if let Err(error) = open_url(APP_REPOSITORY) {
                        write_error_log(&format!("{error:#}"));
                        show_error(&format!("{error:#}"));
                    }
                }
            } else if event.id() == ui.exit_item.id() {
                event_loop.exit();
            }
        }
    }
}

impl ApplicationHandler for App {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if cause == StartCause::Init {
            if let Err(error) = self.initialize_tray() {
                write_error_log(&format!("{error:#}"));
                show_error(&format!("{error:#}"));
                event_loop.exit();
                return;
            }

            self.refresh_status();
        }
    }

    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.process_menu_events(event_loop);

        if Instant::now() >= self.next_refresh {
            self.refresh_status();
        }

        // A short wait keeps the menu responsive without busy-looping.
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(200),
        ));
    }
}

fn make_status_icon(state: &MySqlState) -> Result<Icon> {
    const SIZE: u32 = 32;
    const SCALE: u32 = 4;
    const CANVAS: u32 = SIZE * SCALE;

    let mut high_res = vec![0_u8; (CANVAS * CANVAS * 4) as usize];

    let state_color = state.color();
    let dark = [15, 23, 42, 255];
    let white = [248, 250, 252, 255];
    let highlight = [255, 255, 255, 115];

    draw_database(&mut high_res, CANVAS, 8, 9, 119, 119, 31, white);
    draw_database(&mut high_res, CANVAS, 13, 14, 114, 114, 27, dark);
    draw_database(&mut high_res, CANVAS, 20, 21, 107, 107, 22, state_color);

    draw_filled_ellipse(
        &mut high_res,
        CANVAS,
        28,
        25,
        99,
        53,
        lighten(state_color, 38),
    );

    draw_arc(&mut high_res, CANVAS, 21, 50, 106, 76, highlight, 5);
    draw_arc(&mut high_res, CANVAS, 21, 76, 106, 102, highlight, 5);

    draw_rounded_rect(
        &mut high_res,
        CANVAS,
        28,
        42,
        34,
        91,
        3,
        [255, 255, 255, 70],
    );

    draw_filled_circle(&mut high_res, CANVAS, 98, 99, 27, white);
    draw_filled_circle(&mut high_res, CANVAS, 98, 99, 21, darken(state_color, 12));
    draw_status_symbol(&mut high_res, CANVAS, state);

    let rgba = downsample_rgba(&high_res, CANVAS, SIZE, SCALE);
    Icon::from_rgba(rgba, SIZE, SIZE).context("Could not generate the tray icon")
}

fn draw_status_symbol(buffer: &mut [u8], width: u32, state: &MySqlState) {
    let white = [255, 255, 255, 255];

    match state {
        MySqlState::Running => {
            draw_thick_line(buffer, width, 87, 99, 95, 107, 5, white);
            draw_thick_line(buffer, width, 95, 107, 109, 90, 5, white);
        }
        MySqlState::Stopped => {
            draw_rounded_rect(buffer, width, 89, 90, 107, 108, 3, white);
        }
        MySqlState::Starting | MySqlState::Stopping | MySqlState::Pending => {
            draw_filled_circle(buffer, width, 89, 99, 3, white);
            draw_filled_circle(buffer, width, 98, 99, 3, white);
            draw_filled_circle(buffer, width, 107, 99, 3, white);
        }
        MySqlState::Paused => {
            draw_rounded_rect(buffer, width, 89, 89, 95, 109, 2, white);
            draw_rounded_rect(buffer, width, 101, 89, 107, 109, 2, white);
        }
        MySqlState::NotFound | MySqlState::Error(_) => {
            draw_rounded_rect(buffer, width, 95, 87, 101, 103, 2, white);
            draw_filled_circle(buffer, width, 98, 110, 3, white);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_database(
    buffer: &mut [u8],
    width: u32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    ellipse_height: i32,
    color: [u8; 4],
) {
    let center_x = (left + right) as f32 / 2.0;
    let radius_x = (right - left) as f32 / 2.0;
    let top_center_y = top as f32 + ellipse_height as f32 / 2.0;
    let bottom_center_y = bottom as f32 - ellipse_height as f32 / 2.0;
    let radius_y = ellipse_height as f32 / 2.0;

    for y in top..=bottom {
        for x in left..=right {
            let within_body =
                x >= left && x <= right && y as f32 >= top_center_y && y as f32 <= bottom_center_y;

            let normalized_x = (x as f32 - center_x) / radius_x.max(1.0);
            let top_normalized_y = (y as f32 - top_center_y) / radius_y.max(1.0);
            let bottom_normalized_y = (y as f32 - bottom_center_y) / radius_y.max(1.0);

            let within_top =
                normalized_x * normalized_x + top_normalized_y * top_normalized_y <= 1.0;
            let within_bottom =
                normalized_x * normalized_x + bottom_normalized_y * bottom_normalized_y <= 1.0;

            if within_body || within_top || within_bottom {
                blend_pixel(buffer, width, x, y, color);
            }
        }
    }
}

fn draw_filled_ellipse(
    buffer: &mut [u8],
    width: u32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    color: [u8; 4],
) {
    let center_x = (left + right) as f32 / 2.0;
    let center_y = (top + bottom) as f32 / 2.0;
    let radius_x = (right - left) as f32 / 2.0;
    let radius_y = (bottom - top) as f32 / 2.0;

    for y in top..=bottom {
        for x in left..=right {
            let dx = (x as f32 - center_x) / radius_x.max(1.0);
            let dy = (y as f32 - center_y) / radius_y.max(1.0);

            if dx * dx + dy * dy <= 1.0 {
                blend_pixel(buffer, width, x, y, color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_arc(
    buffer: &mut [u8],
    width: u32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    color: [u8; 4],
    thickness: i32,
) {
    let center_x = (left + right) as f32 / 2.0;
    let center_y = (top + bottom) as f32 / 2.0;
    let radius_x = (right - left) as f32 / 2.0;
    let radius_y = (bottom - top) as f32 / 2.0;

    for x in left..=right {
        let normalized_x = (x as f32 - center_x) / radius_x.max(1.0);
        let under_root = (1.0 - normalized_x * normalized_x).max(0.0);
        let y = center_y + radius_y * under_root.sqrt();

        for offset in -(thickness / 2)..=(thickness / 2) {
            blend_pixel(buffer, width, x, y.round() as i32 + offset, color);
        }
    }
}

fn draw_filled_circle(
    buffer: &mut [u8],
    width: u32,
    center_x: i32,
    center_y: i32,
    radius: i32,
    color: [u8; 4],
) {
    let radius_squared = radius * radius;

    for y in (center_y - radius)..=(center_y + radius) {
        for x in (center_x - radius)..=(center_x + radius) {
            let dx = x - center_x;
            let dy = y - center_y;

            if dx * dx + dy * dy <= radius_squared {
                blend_pixel(buffer, width, x, y, color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_rounded_rect(
    buffer: &mut [u8],
    width: u32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
    color: [u8; 4],
) {
    for y in top..=bottom {
        for x in left..=right {
            let nearest_x = x.clamp(left + radius, right - radius);
            let nearest_y = y.clamp(top + radius, bottom - radius);
            let dx = x - nearest_x;
            let dy = y - nearest_y;

            if dx * dx + dy * dy <= radius * radius {
                blend_pixel(buffer, width, x, y, color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_thick_line(
    buffer: &mut [u8],
    width: u32,
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    thickness: i32,
    color: [u8; 4],
) {
    let dx = end_x - start_x;
    let dy = end_y - start_y;
    let steps = dx.abs().max(dy.abs()).max(1);

    for step in 0..=steps {
        let progress = step as f32 / steps as f32;
        let x = start_x as f32 + dx as f32 * progress;
        let y = start_y as f32 + dy as f32 * progress;
        draw_filled_circle(
            buffer,
            width,
            x.round() as i32,
            y.round() as i32,
            thickness / 2,
            color,
        );
    }
}

fn blend_pixel(buffer: &mut [u8], width: u32, x: i32, y: i32, source: [u8; 4]) {
    if x < 0 || y < 0 || x >= width as i32 || y >= width as i32 {
        return;
    }

    let index = ((y as u32 * width + x as u32) * 4) as usize;
    let source_alpha = source[3] as f32 / 255.0;
    let destination_alpha = buffer[index + 3] as f32 / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);

    if output_alpha <= f32::EPSILON {
        return;
    }

    for channel in 0..3 {
        let source_value = source[channel] as f32 / 255.0;
        let destination_value = buffer[index + channel] as f32 / 255.0;
        let output = (source_value * source_alpha
            + destination_value * destination_alpha * (1.0 - source_alpha))
            / output_alpha;

        buffer[index + channel] = (output * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    buffer[index + 3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn downsample_rgba(source: &[u8], source_size: u32, output_size: u32, scale: u32) -> Vec<u8> {
    let mut output = vec![0_u8; (output_size * output_size * 4) as usize];

    for output_y in 0..output_size {
        for output_x in 0..output_size {
            let mut accumulated = [0_u32; 4];

            for offset_y in 0..scale {
                for offset_x in 0..scale {
                    let source_x = output_x * scale + offset_x;
                    let source_y = output_y * scale + offset_y;
                    let source_index = ((source_y * source_size + source_x) * 4) as usize;

                    for channel in 0..4 {
                        accumulated[channel] += source[source_index + channel] as u32;
                    }
                }
            }

            let samples = scale * scale;
            let output_index = ((output_y * output_size + output_x) * 4) as usize;

            for channel in 0..4 {
                output[output_index + channel] = (accumulated[channel] / samples) as u8;
            }
        }
    }

    output
}

fn lighten(color: [u8; 4], amount: u8) -> [u8; 4] {
    [
        color[0].saturating_add(amount),
        color[1].saturating_add(amount),
        color[2].saturating_add(amount),
        color[3],
    ]
}

fn darken(color: [u8; 4], amount: u8) -> [u8; 4] {
    [
        color[0].saturating_sub(amount),
        color[1].saturating_sub(amount),
        color[2].saturating_sub(amount),
        color[3],
    ]
}

fn make_fallback_icon() -> Icon {
    Icon::from_rgba(vec![127, 140, 141, 255], 1, 1).expect("A 1x1 RGBA icon is always valid")
}

fn launch_elevated_action(action: &str, service_name: &str) -> Result<()> {
    if service_name.contains('"') {
        return Err(anyhow!(
            "The service name contains an invalid quote character"
        ));
    }

    let executable = env::current_exe().context("Could not locate the app executable")?;
    let parameters = format!("--elevated-action {action} --service \"{service_name}\"");

    let operation = to_wide("runas");
    let executable = to_wide(executable.as_os_str());
    let parameters = to_wide(parameters);
    let working_directory = executable_directory_wide()?;

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            executable.as_ptr(),
            parameters.as_ptr(),
            working_directory.as_ptr(),
            SW_HIDE,
        )
    };

    if (result as isize) <= 32 {
        return Err(anyhow!(
            "Windows could not launch the elevated service action (ShellExecuteW code {})",
            result as isize
        ));
    }

    Ok(())
}

fn executable_directory_wide() -> Result<Vec<u16>> {
    let executable = env::current_exe().context("Could not locate the app executable")?;
    let directory = executable
        .parent()
        .ok_or_else(|| anyhow!("The executable has no parent directory"))?;
    Ok(to_wide(directory.as_os_str()))
}

fn to_wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value
        .as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn is_autostart_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(RUN_REGISTRY_KEY, KEY_READ) else {
        return false;
    };

    key.get_value::<String, _>(RUN_VALUE_NAME).is_ok()
}

fn set_autostart(enable: bool) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(RUN_REGISTRY_KEY)
        .context("Could not open the Windows startup registry key")?;

    if enable {
        let executable = env::current_exe().context("Could not locate the app executable")?;
        let command = format!("\"{}\" --startup", executable.display());

        key.set_value(RUN_VALUE_NAME, &command)
            .context("Could not enable startup with Windows")?;
    } else {
        match key.delete_value(RUN_VALUE_NAME) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).context("Could not disable startup with Windows");
            }
        }
    }

    Ok(())
}

fn open_in_notepad(path: &Path) -> Result<()> {
    Command::new("notepad.exe")
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("Could not open Notepad")?;
    Ok(())
}

fn open_windows_services() -> Result<()> {
    Command::new("mmc.exe")
        .arg("services.msc")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("Could not open Windows Services")?;
    Ok(())
}

fn open_url(url: &str) -> Result<()> {
    let operation = to_wide("open");
    let url = to_wide(url);

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };

    if (result as isize) <= 32 {
        return Err(anyhow!(
            "Windows could not open the web browser (ShellExecuteW code {})",
            result as isize
        ));
    }

    Ok(())
}

fn show_about() -> bool {
    let message = format!(
        "{APP_NAME}\n\
         Version {APP_VERSION}\n\n\
         Created by {APP_AUTHORS}\n\n\
         A lightweight Windows tray controller for MySQL and MariaDB services.\n\n\
         {APP_STAR_MESSAGE}\n\
         {APP_REPOSITORY}\n\n\
         Open the GitHub repository now?"
    );

    let title_text = format!("About {APP_NAME}");
    let title = to_wide(title_text.as_str());
    let message = to_wide(message.as_str());

    let result = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONINFORMATION,
        )
    };

    result == IDYES
}

fn show_error(message: &str) {
    let title = to_wide(APP_NAME);
    let message = to_wide(message);

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn shorten(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let shortened: String = chars.by_ref().take(max_chars).collect();

    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}
