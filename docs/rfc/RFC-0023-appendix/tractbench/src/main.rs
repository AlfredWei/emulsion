use std::time::Instant;
use tract_onnx::prelude::*;
use tract_onnx::prelude::tract_ndarray::Dimension;

// usage: tractbench <model.onnx> <name=shape;...> e.g. "input.1=1x3x320x320"
fn main() -> TractResult<()> {
    let a: Vec<String> = std::env::args().collect();
    let path = &a[1];
    let t0 = Instant::now();
    let mut m = tract_onnx::onnx().model_for_path(path)?;
    let mut shapes = vec![];
    for (i, spec) in a[2..].iter().enumerate() {
        let (_n, sh) = spec.split_once('=').unwrap();
        let dims: Vec<usize> = sh.split('x').map(|d| d.parse().unwrap()).collect();
        m = m.with_input_fact(i, f32::fact(&dims).into())?;
        shapes.push(dims);
    }
    let analysed = m.into_optimized();
    let model = match analysed {
        Ok(m) => m,
        Err(e) => { println!("OPTIMIZE FAILED: {e:#}"); return Ok(()); }
    };
    let plan = model.into_runnable()?;
    println!("load+optimize: {:.2}s", t0.elapsed().as_secs_f32());
    let inputs: TVec<TValue> = shapes.iter().map(|d| {
        let n: usize = d.iter().product();
        tract_ndarray::ArrayD::from_shape_fn(d.clone(), |ix| ((ix.slice().iter().sum::<usize>() * 31 % 255) as f32) / 255.0).into_tensor().into()
    }).collect();
    let mut ts = vec![];
    for i in 0..3 {
        let t = Instant::now();
        match plan.run(inputs.clone()) {
            Ok(o) => { ts.push(t.elapsed().as_secs_f32()); if i == 0 { println!("outputs: {:?}", o.iter().map(|x| x.shape().to_vec()).collect::<Vec<_>>()); } }
            Err(e) => { println!("RUN FAILED: {e:#}"); return Ok(()); }
        }
    }
    println!("run times (s): {:?}", ts);
    Ok(())
}
