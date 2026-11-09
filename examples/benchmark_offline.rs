//! Run with SYOUSETU_MODEL_DIR set, in an otherwise idle release build.
//! Reports inference time separately from model verification/loading.
use anyhow::Result;
use sha2::{Digest, Sha256};
use syousetu_translate::{
    learning::Memory,
    models,
    offline::{OfflineTranslator, ParallelPolicy},
};
fn main() -> Result<()> {
    std::env::set_var("CANDLE_NUM_THREADS", "1");
    let source = if let Some(path) = std::env::var_os("SYOUSETU_BENCH_INPUT") {
        std::fs::read_to_string(path)?
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    } else {
        [
            "朝、窓を開けると冷たい風が部屋に入り、遠くの山が白い雲に包まれていた。",
            "彼女は古い図書館で見つけた本を大切に抱えて、静かな道を歩いて帰った。",
            "店の前には多くの人が並んでいたが、私たちは急がずに順番を待つことにした。",
            "雨が降り始めたので、少年は持っていた傘を妹に渡して家まで走って帰った。",
            "新しい仕事を始める前に、必要な資料を確認して同僚と計画について話し合った。",
            "昨日まで閉まっていた扉が開いていて、その向こうから誰かの声が聞こえてきた。",
            "先生は難しい問題を一つずつ説明し、生徒たちはノートを取りながら熱心に聞いていた。",
            "旅の終わりに海辺の小さな村へ着くと、友人たちが笑顔で私を迎えてくれた。",
        ]
        .map(str::to_owned)
        .to_vec()
    };
    let mut baseline = None;
    let mut report = Vec::new();
    for threads in [1, 2, 4] {
        let load = std::time::Instant::now();
        let mut engine = OfflineTranslator::open_with_policy(
            &models::default_dir(),
            Memory::default(),
            threads,
            if threads == 1 {
                ParallelPolicy::Serial
            } else {
                ParallelPolicy::Parallel
            },
        )?;
        let load_seconds = load.elapsed().as_secs_f64();
        engine.translate("準備のための短い文です。")?;
        let start = std::time::Instant::now();
        let output = engine.translate_lines(&source)?;
        let seconds = start.elapsed().as_secs_f64();
        if let Some(expected) = &baseline {
            anyhow::ensure!(&output == expected, "serial/parallel output mismatch");
        } else {
            baseline = Some(output.clone());
        }
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&output)?));
        let result = serde_json::json!({"threads":threads,"lines":source.len(),"load_seconds":load_seconds,"inference_seconds":seconds,"output_sha256":digest});
        println!("{result}");
        report.push(result);
    }
    let serial = report[0]["inference_seconds"].as_f64().unwrap();
    for row in &mut report {
        row["speedup_vs_one_cpu"] =
            serde_json::json!(serial / row["inference_seconds"].as_f64().unwrap());
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
