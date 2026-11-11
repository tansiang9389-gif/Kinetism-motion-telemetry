//! BLE central for the `Physio_A` master sensor (btleplug / WinRT on Windows).

use super::{LinkState, Shared, frame};
use btleplug::api::{Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;
use uuid::Uuid;

pub const DEVICE_NAME: &str = "Physio_A";
pub const SERVICE_UUID: Uuid = Uuid::from_u128(0x4fafc201_1fb5_459e_8fcc_c5c9c331914b);
pub const TELEMETRY_UUID: Uuid = Uuid::from_u128(0xbeb5483e_36e1_4688_b7f5_ea07361b26a8);

const SCAN_TIMEOUT: Duration = Duration::from_secs(12);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const SILENCE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RECONNECTS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleCommand {
    Connect,
    Disconnect,
}

type SharedState = Arc<Mutex<Shared>>;

fn with<R>(shared: &SharedState, f: impl FnOnce(&mut Shared) -> R) -> R {
    let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// Never overwrite simulator mode from the BLE task.
fn set_state(shared: &SharedState, ctx: &eframe::egui::Context, state: LinkState) {
    with(shared, |s| {
        if s.state != LinkState::Simulated {
            s.set_state(state);
        }
    });
    ctx.request_repaint();
}

enum SessionEnd {
    UserDisconnect,
    Dropped(String),
}

pub async fn run(
    shared: SharedState,
    mut commands: UnboundedReceiver<BleCommand>,
    ctx: eframe::egui::Context,
) {
    while let Some(command) = commands.recv().await {
        if command != BleCommand::Connect {
            continue;
        }
        let mut attempts = 0;
        loop {
            let started = tokio::time::Instant::now();
            let outcome = session(&shared, &mut commands, &ctx).await;
            if started.elapsed() > Duration::from_secs(10) {
                attempts = 0;
            }
            match outcome {
                Ok(SessionEnd::UserDisconnect) => {
                    set_state(&shared, &ctx, LinkState::Idle);
                    break;
                }
                Ok(SessionEnd::Dropped(reason)) if attempts < MAX_RECONNECTS => {
                    attempts += 1;
                    set_state(
                        &shared,
                        &ctx,
                        LinkState::Reconnecting(format!(
                            "{reason} - retry {attempts}/{MAX_RECONNECTS}"
                        )),
                    );
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    if matches!(commands.try_recv(), Ok(BleCommand::Disconnect)) {
                        set_state(&shared, &ctx, LinkState::Idle);
                        break;
                    }
                }
                Ok(SessionEnd::Dropped(reason)) => {
                    set_state(&shared, &ctx, LinkState::Error(reason));
                    break;
                }
                Err(error) => {
                    set_state(&shared, &ctx, LinkState::Error(format!("{error:#}")));
                    break;
                }
            }
        }
    }
}

async fn first_adapter() -> anyhow::Result<Adapter> {
    let manager = Manager::new().await?;
    manager.adapters().await?.into_iter().next().ok_or_else(|| {
        anyhow::anyhow!("No Bluetooth adapter found. Turn Bluetooth on in Windows settings.")
    })
}

async fn find_device(
    adapter: &Adapter,
    commands: &mut UnboundedReceiver<BleCommand>,
) -> anyhow::Result<Option<(Peripheral, String)>> {
    adapter.start_scan(ScanFilter::default()).await?;
    let started = tokio::time::Instant::now();
    loop {
        if matches!(commands.try_recv(), Ok(BleCommand::Disconnect)) {
            let _ = adapter.stop_scan().await;
            return Ok(None);
        }
        for peripheral in adapter.peripherals().await? {
            let Ok(Some(props)) = peripheral.properties().await else {
                continue;
            };
            let name = props
                .local_name
                .clone()
                .or(props.advertisement_name.clone())
                .unwrap_or_default();
            if name.starts_with(DEVICE_NAME) || props.services.contains(&SERVICE_UUID) {
                let _ = adapter.stop_scan().await;
                let label = if name.is_empty() {
                    DEVICE_NAME.to_string()
                } else {
                    name
                };
                return Ok(Some((peripheral, label)));
            }
        }
        if started.elapsed() > SCAN_TIMEOUT {
            let _ = adapter.stop_scan().await;
            anyhow::bail!(
                "{DEVICE_NAME} not found. Switch the sensor on and disconnect it from the phone app (a connected sensor stops advertising)."
            );
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

async fn session(
    shared: &SharedState,
    commands: &mut UnboundedReceiver<BleCommand>,
    ctx: &eframe::egui::Context,
) -> anyhow::Result<SessionEnd> {
    set_state(shared, ctx, LinkState::Scanning);
    let adapter = first_adapter().await?;
    let Some((peripheral, name)) = find_device(&adapter, commands).await? else {
        return Ok(SessionEnd::UserDisconnect);
    };
    set_state(shared, ctx, LinkState::Connecting(name.clone()));
    if !peripheral.is_connected().await.unwrap_or(false) {
        tokio::time::timeout(CONNECT_TIMEOUT, peripheral.connect())
            .await
            .map_err(|_| anyhow::anyhow!("Timed out connecting to {name}."))??;
    }
    peripheral.discover_services().await?;
    let telemetry = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == TELEMETRY_UUID)
        .ok_or_else(|| {
            anyhow::anyhow!("Telemetry characteristic missing: check the master firmware.")
        })?;
    if !telemetry.properties.contains(CharPropFlags::NOTIFY) {
        anyhow::bail!("Telemetry characteristic does not support notifications.");
    }
    let mut notifications = peripheral.notifications().await?;
    peripheral.subscribe(&telemetry).await?;
    set_state(shared, ctx, LinkState::Streaming(name.clone()));

    let mut waiting = false;
    loop {
        tokio::select! {
            command = commands.recv() => {
                if matches!(command, Some(BleCommand::Disconnect) | None) {
                    let _ = peripheral.unsubscribe(&telemetry).await;
                    let _ = peripheral.disconnect().await;
                    return Ok(SessionEnd::UserDisconnect);
                }
            }
            next = tokio::time::timeout(SILENCE_TIMEOUT, notifications.next()) => {
                match next {
                    Ok(Some(notification)) if notification.uuid == TELEMETRY_UUID => {
                        with(shared, |s| match frame::parse_frame(&notification.value) {
                            Ok(f) => s.push(f),
                            Err(e) => s.invalid(e.to_string()),
                        });
                        if waiting {
                            waiting = false;
                            set_state(shared, ctx, LinkState::Streaming(name.clone()));
                        }
                        ctx.request_repaint();
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => return Ok(SessionEnd::Dropped("Sensor disconnected".into())),
                    Err(_) => {
                        if !peripheral.is_connected().await.unwrap_or(false) {
                            return Ok(SessionEnd::Dropped("Sensor disconnected".into()));
                        }
                        waiting = true;
                        set_state(shared, ctx, LinkState::Connecting(format!("{name}: connected, waiting for data")));
                    }
                }
            }
        }
    }
}
