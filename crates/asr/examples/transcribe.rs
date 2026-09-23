//! 转写一个 WAV 文件：cargo run -p asr --example transcribe -- <wav路径>
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wav = std::env::args().nth(1).unwrap_or_else(|| {
        "data/models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/test_wavs/zh.wav".into()
    });

    println!("加载模型…");
    let t0 = Instant::now();
    let mut engine = asr::AsrEngine::open_default()?;
    println!("模型加载：{:.2?}", t0.elapsed());

    println!("转写 {wav} …");
    let t1 = Instant::now();
    let result = engine.transcribe_wav(&wav)?;
    println!("推理耗时：{:.2?}", t1.elapsed());
    println!("语言：{}", result.lang);
    println!("全文：{}", result.text);
    println!("句段数：{}", result.segments.len());
    for seg in result.segments.iter().take(5) {
        println!("  [{:>6.2} - {:>6.2}] {}", seg.start, seg.end, seg.text);
    }
    Ok(())
}
