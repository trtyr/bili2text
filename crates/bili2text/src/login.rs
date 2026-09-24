//! 账号：扫码登录（终端二维码）/ 登出 / 状态。

use std::time::Duration;

use bili_client::{BiliClient, qrcode};

use crate::error::AppError;

type Result<T> = std::result::Result<T, AppError>;

/// 扫码登录：终端渲染二维码 → 轮询 → 登录态落盘。
/// 二维码失效自动重新出码，直到成功或用户 Ctrl-C。
pub async fn login(client: &BiliClient) -> Result<()> {
    loop {
        let qr = client.qrcode_generate().await.map_err(AppError::from_bili)?;
        println!("请用哔哩哔哩 App 扫码登录（约 3 分钟内有效）：\n");
        print_qr(&qr.url).map_err(|e| AppError::Other(format!("渲染二维码失败：{e}")))?;
        println!();

        let mut prev: Option<i64> = None;
        loop {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let poll = client
                .qrcode_poll(&qr.qrcode_key)
                .await
                .map_err(AppError::from_bili)?;
            if Some(poll.code) != prev {
                match poll.code {
                    qrcode::POLL_SCANNED => println!("· 已扫码，请在手机上确认…"),
                    qrcode::POLL_EXPIRED => println!("· 二维码已失效，重新生成…"),
                    _ => {}
                }
                prev = Some(poll.code);
            }
            if poll.code == qrcode::POLL_SUCCESS {
                let cred = match poll.credential {
                    Some(c) => c,
                    None => {
                        return Err(AppError::Other(
                            "B 站返回登录成功但未下发登录态（协议异常）".into(),
                        ));
                    }
                };
                client.set_credential(cred).map_err(AppError::from_bili)?;
                println!("✓ 登录成功，登录态已保存");
                return Ok(());
            }
            if poll.code == qrcode::POLL_EXPIRED {
                break; // 换新码
            }
        }
    }
}

/// 登出：清除本机登录态。
pub async fn logout(client: &BiliClient) -> Result<()> {
    client.clear_credential().map_err(AppError::from_bili)?;
    println!("✓ 已退出登录");
    Ok(())
}

/// 查看登录态（真实请求 B 站校验有效性）。
pub async fn status(client: &BiliClient) -> Result<()> {
    let has = client.credential().header_value().is_some();
    if !has {
        println!("未登录。AI 字幕与高音质下载需要登录：bili2text login");
        return Ok(());
    }
    let ok = client.is_logged_in().await;
    if ok {
        println!("✓ 已登录（登录态有效）");
    } else {
        println!("✗ 本机有登录态存档，但已失效。请重新登录：bili2text login");
    }
    Ok(())
}

/// 终端渲染二维码（UTF-8 半块字符；黑底终端下前景块即暗模块）。
fn print_qr(content: &str) -> anyhow::Result<()> {
    use qrencode::{Color, QrCode};
    let code = QrCode::new(content.as_bytes())?;
    let w = code.width() as i32;
    let quiet = 2i32; // 四周留白模块，便于摄像头识别
    let dark = |x: i32, y: i32| {
        x >= 0
            && x < w
            && y >= 0
            && y < w
            && code[(x as usize, y as usize)] == Color::Dark
    };
    let mut out = String::new();
    let mut y = -quiet;
    while y < w + quiet {
        let mut line = String::new();
        let mut x = -quiet;
        while x < w + quiet {
            line.push(match (dark(x, y), dark(x, y + 1)) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
            x += 1;
        }
        out.push_str(line.trim_end());
        out.push('\n');
        y += 2;
    }
    print!("{out}");
    Ok(())
}
