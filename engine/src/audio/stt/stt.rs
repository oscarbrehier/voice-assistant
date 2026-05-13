// use std::{path::Path, time::Instant};

// use anyhow::Context;
// use ndarray::{Array2, Array3, s};
// use ort::session::Session;
// use tokenizers::Tokenizer;
// use toml::value::Array;

// pub struct STT {
//     encoder: Session,
//     decoder: Session,
//     tokenizer: Tokenizer,
// }

// impl STT {
//     pub fn new(model_dir: &str) -> anyhow::Result<Self> {
//         let path = Path::new(model_dir);
//         let encoder_path = path.join("encoder_model.onnx");
//         let decoder_path = path.join("decoder_model.onnx");
//         let tokenizer_path = path.join("tokenizer.json");

//         let encoder = Session::builder()?.commit_from_file(encoder_path)?;
//         let decoder = Session::builder()?.commit_from_file(decoder_path)?;
//         let tokenizer =
//             tokenizers::Tokenizer::from_file(tokenizer_path).map_err(anyhow::Error::msg)?;

//         Ok(Self {
//             encoder,
//             decoder,
//             tokenizer,
//         })
//     }

//     // pub fn transcribe(&mut self, samples: &[f32]) -> anyhow::Result<String> {
//     //     let total_start = Instant::now();

//     //     println!("Starting transcription for {} samples", samples.len());

//     //     let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

//     //     params.set_n_threads(4);
//     //     params.set_print_special(false);
//     //     params.set_print_progress(false);
//     //     params.set_print_realtime(false);
//     //     params.set_print_timestamps(false);

//     //     params.set_translate(false);
//     //     params.set_language(Some("en"));
//     //     params.set_no_context(true);

//     //     params.set_max_len(0);

//     //     self.state
//     //         .full(params, samples)
//     //         .context("Failed to run whisper inference")?;

//     //     let mut result = String::new();

//     //     for segment in self.state.as_iter() {
//     //         result.push_str(&segment.to_string());
//     //     }

//     //     let total_time = total_start.elapsed();
//     //     println!(
//     //         "Transcription complete: '{}' ({} samples in {:?})",
//     //         result.trim(),
//     //         samples.len(),
//     //         total_time
//     //     );

//     //     Ok(result.trim().to_string())
//     // }

//     pub fn transcribe(&mut self, samples: &[f32]) -> anyhow::Result<String> {
//         let mel = self.audio_to_mel(samples)?;

//         let mel_value = ort::value::Value::from_array(mel)?;
//         let encoder_out = self
//             .encoder
//             .run(ort::inputs!["input_features" => mel_value])?;

//         let (hidden_shape, hidden_slice) =
//             encoder_out["last_hidden_state"].try_extract_tensor::<f32>()?;
//         let shape_array = [
//             hidden_shape[0] as usize,
//             hidden_shape[1] as usize,
//             hidden_shape[2] as usize,
//         ];

//         let hidden_owned = ndarray::ArrayView3::from_shape(shape_array, hidden_slice)?.to_owned();
//         let hidden_states_value = ort::value::Value::from_array(hidden_owned)?;

//         let mut tokens = vec![50258, 50259, 50359, 50363];
//         let max_tokens = 448;

//         for _ in 0..max_tokens {
//             let decoder_input_array =
//                 Array2::from_shape_vec((1, tokens.len()), tokens.clone())?.mapv(|x| x as i64);

//             let input_ids_value = ort::value::Value::from_array(decoder_input_array)?;

//             let decoder_outputs = self.decoder.run(ort::inputs![
//                 "input_ids" => input_ids_value,
//                 "encoder_hidden_states" => &hidden_states_value
//             ])?;

//             let (logits_shape, logits_slice) =
//                 decoder_outputs["logits"].try_extract_tensor::<f32>()?;
//             let logits_view = ndarray::ArrayView3::from_shape(
//                 [
//                     logits_shape[0] as usize,
//                     logits_shape[1] as usize,
//                     logits_shape[2] as usize,
//                 ],
//                 logits_slice,
//             )?;

//             let last_token_logits = logits_view.slice(s![0, -1, ..]);

//             let next_token = last_token_logits
//                 .iter()
//                 .enumerate()
//                 .max_by(|(_, a): &(usize, &f32), (_, b): &(usize, &f32)| {
//                     a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
//                 })
//                 .map(|(index, _)| index as u32)
//                 .ok_or_else(|| anyhow::anyhow!("Logits empty"))?;

//             if next_token == 50257 {
//                 break;
//             }

//             tokens.push(next_token);
//         }

//         let text = self
//             .tokenizer
//             .decode(&tokens, true)
//             .map_err(|e| anyhow::anyhow!(e))?;

//         Ok(text)
//     }

//     fn audio_to_mel(&self, _audio: &[f32]) -> anyhow::Result<Array3<f32>> {
//         Ok(Array3::zeros((1, 80, 3000)))
//     }
// }
