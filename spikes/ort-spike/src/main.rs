//! M6 slice 0 spike (RFC-0023 / ADR-0009): does `ort` link and run on the CI
//! platforms, and how fast is it? Usage:
//!   ort-spike <model.onnx> <cpu|coreml|directml> <name=1x3x320x320> [<name=shape> ...]
//! All inputs are f32 with a deterministic non-constant fill. Prints one JSON
//! line (also on failure) so CI logs are greppable. Throwaway, not product code.

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::time::Instant;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(args: &[String]) -> Result<String, String> {
    let model = &args[1];
    let provider = args[2].as_str();
    let t = Instant::now();
    let mut builder = Session::builder()
        .map_err(|e| format!("builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("opt level: {e}"))?;
    builder = match provider {
        "cpu" => builder,
        #[cfg(target_os = "macos")]
        "coreml" => builder
            .with_execution_providers([ort::ep::CoreML::default().build().error_on_failure()])
            .map_err(|e| format!("coreml EP: {e}"))?,
        #[cfg(windows)]
        "directml" => builder
            .with_execution_providers([ort::ep::DirectML::default().build().error_on_failure()])
            .map_err(|e| format!("directml EP: {e}"))?,
        other => return Err(format!("provider `{other}` not available on this platform build")),
    };
    let mut session = builder.commit_from_file(model).map_err(|e| format!("load model: {e}"))?;
    let load_ms = ms(t);

    let mut inputs: Vec<(String, Tensor<f32>)> = Vec::new();
    for spec in &args[3..] {
        let (name, shape) = spec.split_once('=').ok_or("bad input spec")?;
        let dims: Vec<usize> = shape.split('x').map(|d| d.parse().unwrap()).collect();
        let n: usize = dims.iter().product();
        let data: Vec<f32> = (0..n).map(|i| ((i.wrapping_mul(2654435761) >> 8) % 1000) as f32 / 1000.0).collect();
        inputs.push((name.to_string(), Tensor::from_array((dims, data)).map_err(|e| format!("tensor: {e}"))?));
    }

    let mut times = Vec::new();
    let mut out_shapes = String::new();
    for i in 0..4 {
        let feed: Vec<(String, ort::session::SessionInputValue<'_>)> =
            inputs.iter().map(|(n, t)| (n.clone(), ort::session::SessionInputValue::from(t.view()))).collect();
        let t = Instant::now();
        let outs = session.run(feed).map_err(|e| format!("run: {e}"))?;
        times.push(ms(t));
        if i == 0 {
            out_shapes = outs
                .iter()
                .map(|(n, v)| format!("{n}:{:?}", v.shape()))
                .collect::<Vec<_>>()
                .join(",");
        }
    }
    let first = times[0];
    let mut rest = times[1..].to_vec();
    rest.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Ok(format!(
        "\"ok\":true,\"load_ms\":{load_ms:.1},\"first_run_ms\":{first:.1},\"median_ms\":{:.1},\"outputs\":\"{out_shapes}\"",
        rest[rest.len() / 2]
    ))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: ort-spike <model.onnx> <cpu|coreml|directml> <name=shape> ...");
        std::process::exit(2);
    }
    let body = match run(&args) {
        Ok(b) => b,
        Err(e) => format!("\"ok\":false,\"error\":{:?}", e),
    };
    println!(
        "ORTSPIKE {{\"os\":\"{}\",\"model\":{:?},\"provider\":\"{}\",{body}}}",
        std::env::consts::OS,
        std::path::Path::new(&args[1]).file_name().unwrap().to_string_lossy(),
        args[2]
    );
}
