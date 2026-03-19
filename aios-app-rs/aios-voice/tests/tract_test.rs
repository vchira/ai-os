use std::path::PathBuf;
use tract_onnx::prelude::*;

fn models_dir() -> Option<PathBuf> {
    let paths: Vec<PathBuf> = vec![
        PathBuf::from("/opt/aios-app/models/kws"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .parent().unwrap()
            .join("distro/build/chroot/opt/aios-app/models/kws"),
    ];
    paths.into_iter().find(|p: &PathBuf| p.join("infrastructure/melspectrogram.onnx").exists())
}

#[test]
fn tract_loads_melspectrogram() {
    let Some(dir) = models_dir() else { eprintln!("SKIP: no models"); return; };
    let path = dir.join("infrastructure/melspectrogram.onnx");
    eprintln!("Loading {}", path.display());
    let model = tract_onnx::onnx().model_for_path(&path).expect("load mel failed");
    eprintln!("OK — inputs: {:?}", model.input_outlets());
}

#[test]
fn tract_loads_embedding() {
    let Some(dir) = models_dir() else { return; };
    let path = dir.join("infrastructure/embedding_model.onnx");
    eprintln!("Loading {}", path.display());
    let model = tract_onnx::onnx().model_for_path(&path).expect("load emb failed");
    eprintln!("OK — inputs: {:?}", model.input_outlets());
}

#[test]
fn tract_loads_hey_jarvis() {
    let Some(dir) = models_dir() else { return; };
    let path = dir.join("pretrained/hey_jarvis.onnx");
    eprintln!("Loading {}", path.display());
    let model = tract_onnx::onnx().model_for_path(&path).expect("load hey_jarvis failed");
    eprintln!("OK — inputs: {:?}", model.input_outlets());
}

#[test]
fn tract_runs_inference_on_hey_jarvis() {
    let Some(dir) = models_dir() else { return; };
    
    // Load and optimize the hey_jarvis model
    let path = dir.join("pretrained/hey_jarvis.onnx");
    let model = tract_onnx::onnx()
        .model_for_path(&path)
        .expect("load model");
    
    // Get input shape info
    let input_fact = model.input_fact(0).expect("input fact");
    eprintln!("Input fact: {:?}", input_fact);
    
    // The wake word model expects embeddings, not raw audio.
    // For now just verify the model loads and we can query its shape.
    eprintln!("Model loaded and queryable");
}

#[test]
fn inspect_all_model_shapes() {
    let Some(dir) = models_dir() else { return; };
    
    // Mel model
    let mel = tract_onnx::onnx().model_for_path(dir.join("infrastructure/melspectrogram.onnx")).unwrap();
    eprintln!("MEL inputs:");
    for (i, inlet) in mel.input_outlets().unwrap().iter().enumerate() {
        eprintln!("  [{i}]: {:?}", mel.input_fact(i));
    }
    eprintln!("MEL outputs:");
    for (i, _) in mel.output_outlets().unwrap().iter().enumerate() {
        eprintln!("  [{i}]: {:?}", mel.output_fact(i));
    }

    // Embedding model
    let emb = tract_onnx::onnx().model_for_path(dir.join("infrastructure/embedding_model.onnx")).unwrap();
    eprintln!("EMB inputs:");
    for (i, _) in emb.input_outlets().unwrap().iter().enumerate() {
        eprintln!("  [{i}]: {:?}", emb.input_fact(i));
    }
    eprintln!("EMB outputs:");
    for (i, _) in emb.output_outlets().unwrap().iter().enumerate() {
        eprintln!("  [{i}]: {:?}", emb.output_fact(i));
    }

    // Wake word model
    let ww = tract_onnx::onnx().model_for_path(dir.join("pretrained/hey_jarvis.onnx")).unwrap();
    eprintln!("WW inputs:");
    for (i, _) in ww.input_outlets().unwrap().iter().enumerate() {
        eprintln!("  [{i}]: {:?}", ww.input_fact(i));
    }
    eprintln!("WW outputs:");
    for (i, _) in ww.output_outlets().unwrap().iter().enumerate() {
        eprintln!("  [{i}]: {:?}", ww.output_fact(i));
    }
}

#[test]
fn run_mel_inference() {
    let Some(dir) = models_dir() else { return; };
    
    let mut mel = tract_onnx::onnx().model_for_path(dir.join("infrastructure/melspectrogram.onnx")).unwrap();
    // Set concrete input: [1, 1280]
    mel.set_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), &[1, 1280])).unwrap();
    let plan = SimplePlan::new(mel).unwrap();
    
    let input = tract_ndarray::Array2::<f32>::zeros((1, 1280));
    let result = plan.run(tvec![input.into_tvalue()]).unwrap();
    eprintln!("MEL output shape: {:?}, len: {}", result[0].shape(), result[0].len());
}

#[test]
fn run_embedding_inference() {
    let Some(dir) = models_dir() else { return; };
    
    // First get mel output to understand the shape
    let mut mel = tract_onnx::onnx().model_for_path(dir.join("infrastructure/melspectrogram.onnx")).unwrap();
    mel.set_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), &[1, 1280])).unwrap();
    let mel_plan = SimplePlan::new(mel).unwrap();
    
    // Run 76 chunks of mel (76 * 80ms = 6.08s — about what openWakeWord accumulates)
    let mut mel_features: Vec<f32> = Vec::new();
    for _ in 0..76 {
        let input = tract_ndarray::Array2::<f32>::zeros((1, 1280));
        let result = mel_plan.run(tvec![input.into_tvalue()]).unwrap();
        let data: Vec<f32> = result[0].to_array_view::<f32>().unwrap().iter().copied().collect();
        mel_features.extend(data);
    }
    eprintln!("Accumulated mel features: {} values", mel_features.len());
    // 76 chunks * 160 features = 12160 values
    // But embedding expects a specific 2D shape...
    
    // Try [1, n_frames, 32] shape — mel output is [1,1,5,32] per chunk
    // So 76 chunks = [1, 76*5, 32] = [1, 380, 32]
    let n_frames = 76 * 5;
    eprintln!("Trying embedding input shape: [1, {n_frames}, 32]");
    
    let mut emb = tract_onnx::onnx().model_for_path(dir.join("infrastructure/embedding_model.onnx")).unwrap();
    emb.set_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), &[1, n_frames, 32])).unwrap();
    let emb_plan = SimplePlan::new(emb).unwrap();
    
    let input = tract_ndarray::Array3::<f32>::from_shape_vec((1, n_frames, 32), mel_features).unwrap();
    let result = emb_plan.run(tvec![input.into_tvalue()]).unwrap();
    eprintln!("EMB output shape: {:?}, len: {}", result[0].shape(), result[0].len());
}
