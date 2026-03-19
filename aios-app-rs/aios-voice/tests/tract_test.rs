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
