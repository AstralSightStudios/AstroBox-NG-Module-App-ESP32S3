use corelib::device::{
    self,
    xiaomi::{SendError, r#type::ConnectType},
};
use esp32_nimble::{BLEDevice, BLEScan, utilities::BleUuid, utilities::BleUuid::Uuid16};
use log::info;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub mod ancs;

fn u16_uuid(u: u16) -> BleUuid {
    BleUuid::from(Uuid16(u))
}
fn uuid_contains(u: &BleUuid, needle: &str) -> bool {
    let s = format!("{u:?}").replace('-', "").to_ascii_lowercase();
    s.contains(&needle.to_ascii_lowercase())
}

pub async fn connect() -> anyhow::Result<()> {
    let mi_service = u16_uuid(0xFE95);
    let uuid_service_flag = u16_uuid(0x0050);
    let uuid_recv = u16_uuid(0x005E);
    let uuid_sent = u16_uuid(0x005F);

    let ble = BLEDevice::take();
    ancs::init_fake_ancs_service(&mut *ble)?;
    let handle = tokio::runtime::Handle::current();

    let mut scan = BLEScan::new();
    scan.active_scan(true).interval(80).window(40);

    let wanted_name = "Xiaomi Watch S4";
    info!("Start scanning...");
    let addr = scan
        .start(&ble, 10_000, |dev, adv| {
            let hit = adv
                .name()
                .map(|n| n.to_string().contains(wanted_name))
                .unwrap_or(false)
                || adv.service_uuids().any(|u| u == mi_service);

            if hit {
                info!("Found target: {:?} rssi={}", adv.name(), dev.rssi());
                Some(dev.addr())
            } else {
                None
            }
        })
        .await?
        .ok_or_else(|| anyhow::anyhow!("Target device not found"))?;
    info!("Target addr = {addr}");

    let mut client: esp32_nimble::BLEClient = ble.new_client();
    client.set_connection_params(12, 24, 0, 400, 16, 16);
    info!("Connecting...");
    client.connect(&addr).await?;
    info!("Connected = {}", client.connected());

    let svc = client
        .get_service(mi_service)
        .await
        .map_err(|_| anyhow::anyhow!("Can't found fe95 service"))?;

    let mut ch_service_flag = None;
    let mut ch_recv = None;
    let mut ch_sent = None;

    let chars: Vec<_> = svc.get_characteristics().await?.collect();

    for c in &chars {
        let u = c.uuid();
        if ch_recv.is_none() && u == uuid_recv {
            ch_recv = Some((*c).clone());
            continue;
        }
        if ch_sent.is_none() && u == uuid_sent {
            ch_sent = Some((*c).clone());
            continue;
        }
        if ch_service_flag.is_none() && u == uuid_service_flag {
            ch_service_flag = Some((*c).clone());
            continue;
        }
    }

    if ch_service_flag.is_none() || ch_recv.is_none() || ch_sent.is_none() {
        for c in &chars {
            let u = c.uuid();
            if ch_recv.is_none() && uuid_contains(&u, "005e") {
                ch_recv = Some((*c).clone());
                continue;
            }
            if ch_sent.is_none() && uuid_contains(&u, "005f") {
                ch_sent = Some((*c).clone());
                continue;
            }
            if ch_service_flag.is_none() && uuid_contains(&u, "0050") {
                ch_service_flag = Some((*c).clone());
                continue;
            }
        }
    }

    let mut ch_service_flag = ch_service_flag.ok_or_else(|| anyhow::anyhow!("0x0050 not found"))?;
    let mut ch_recv = ch_recv.ok_or_else(|| anyhow::anyhow!("0x005e not found"))?;
    let ch_sent = ch_sent.ok_or_else(|| anyhow::anyhow!("0x005f not found"))?;

    if ch_service_flag.can_read() {
        if let Ok(v) = ch_service_flag.read_value().await {
            info!("Read 0x0050 = {:02X?}", v);
        }
    }

    let (tx, mut rx) =
        mpsc::unbounded_channel::<(Vec<u8>, oneshot::Sender<Result<(), SendError>>)>();
    let mut ch_sent_worker = ch_sent;
    let _send_task = tokio::task::spawn_local(async move {
        while let Some((data, responder)) = rx.recv().await {
            let result: Result<(), SendError> = async {
                if ch_sent_worker.can_write() {
                    ch_sent_worker
                        .write_value(&data, true)
                        .await
                        .map_err(|e| SendError::Io(e.to_string()))?;
                } else if ch_sent_worker.can_write_no_response() {
                    ch_sent_worker
                        .write_value(&data, false)
                        .await
                        .map_err(|e| SendError::Io(e.to_string()))?;
                } else {
                    return Err(SendError::Io("0x005F can't write".to_string()));
                }
                Ok(())
            }
            .await;
            let _ = responder.send(result);
        }
    });

    let tx = Arc::new(tx);
    let send_cb = {
        let tx = Arc::clone(&tx);
        move |data: Vec<u8>| {
            let tx = Arc::clone(&tx);
            async move {
                let (resp_tx, resp_rx) = oneshot::channel();
                tx.send((data, resp_tx))
                    .map_err(|_| SendError::Io("send queue closed".to_string()))?;
                resp_rx
                    .await
                    .map_err(|_| SendError::Io("send task dropped".to_string()))?
            }
        }
    };

    let device_addr = addr.to_string();
    let device_name = wanted_name.to_string();
    let auth_key = "bec25af5568a0121bbde9a768ec2d7f9".to_string();
    let sar_version = 2;

    if ch_recv.can_notify() {
        let notify_handle = handle.clone();
        let notify_addr = device_addr.clone();
        ch_recv.on_notify(move |payload| {
            log::info!("Notify(0x005E): {}", corelib::tools::to_hex_string(payload));
            corelib::device::xiaomi::packet::dispatcher::on_packet(
                notify_handle.clone(),
                notify_addr.clone(),
                payload.to_vec(),
            );
        });
        ch_recv.subscribe_notify(true).await?;
        info!("Subscribed notify on 0x005E");
    } else {
        info!("0x005E doesn't support Notify");
    }

    device::create_miwear_device(
        handle.clone(),
        device_name,
        device_addr.clone(),
        auth_key,
        sar_version,
        ConnectType::BLE,
        false,
        move |data| {
            log::info!("Write(0x005F): {}", corelib::tools::to_hex_string(&data));
            let fut = send_cb(data);
            async move {
                fut.await.map_err(|err| {
                    log::error!("send failed: {:?}", err);
                    err
                })
            }
        },
    )
    .await?;

    Ok(())
}
