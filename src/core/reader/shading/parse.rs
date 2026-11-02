//! Shading 资源值与页面 `/Shading` 资源的解析入口，及 Type 1–3 简单渐变
//! 与 Type 4–7 网格渐变的字典/流解析。

use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::types::{Dict, Document, Value};

use super::bitstream::{decode_mesh, group_patches, group_tensor_patches};
use super::error::{MAX_MESH_RECORDS, Result, malformed, unsupported};
use super::model::{
    AxialShading, CoonsPatchShading, FreeFormMeshShading, FunctionShading, LatticeMeshShading,
    RadialShading, ShadingModel, TensorProductPatchShading,
};

impl Document {
    /// 从 Shading 资源值（字典、流或间接引用）构造类型化模型。
    pub fn shading(&self, value: &Value) -> Result<ShadingModel> {
        self.shading_model(value)
    }

    /// 解析页面 `/Resources/Shading` 中的命名着色；资源不存在返回 `None`。
    pub fn page_shading(&self, page_index: usize, name: &str) -> Result<Option<ShadingModel>> {
        let value = self
            .page_resource(page_index, "Shading", name)
            .map_err(|error| malformed(error.to_string()))?;
        match value {
            Some(value) => self.shading_model(&value).map(Some),
            None => Ok(None),
        }
    }
    pub(super) fn shading_model(&self, value: &Value) -> Result<ShadingModel> {
        let resolved = self.resolve(value)?;
        let (dict, data): (Dict, Option<Vec<u8>>) = match &resolved {
            Value::Dict(_) => (resolved.as_dict().cloned().expect("dict"), None),
            Value::Stream(_, _) => {
                let dict = resolved.as_dict().cloned().expect("stream dict");
                let bytes = self
                    .decode_stream_bytes(&resolved)
                    .map_err(|error| malformed(format!("shading stream: {error}")))?;
                (dict, Some(bytes))
            }
            other => {
                return Err(malformed(format!(
                    "shading must be a dictionary or stream, got {}",
                    other.type_name()
                )));
            }
        };
        let kind = dict
            .get("ShadingType")
            .and_then(Value::as_integer)
            .ok_or_else(|| malformed("missing required key /ShadingType"))?;
        match kind {
            1..=3 => {
                let model = self.parse_simple_shading(kind, &dict)?;
                Ok(model)
            }
            4..=7 => {
                let bytes = data.ok_or_else(|| {
                    malformed(format!("shading type {kind} requires stream data"))
                })?;
                self.parse_mesh_shading(kind, &dict, &bytes)
            }
            other => Err(unsupported(format!("unsupported /ShadingType {other}"))),
        }
    }

    fn parse_simple_shading(&self, kind: i64, dict: &Dict) -> Result<ShadingModel> {
        let color_space = self.shading_color_space(dict)?;
        let components = color_space
            .component_count()
            .ok_or_else(|| unsupported("shading color space has no fixed component count"))?;
        let function = self.shading_function(dict)?;
        let inputs = function.domain().len();
        let expected_inputs = if kind == 1 { 2 } else { 1 };
        if inputs != expected_inputs {
            return Err(malformed(format!(
                "shading type {kind} function must take {expected_inputs} input, got {inputs}"
            )));
        }
        if let Some(outputs) = function.output_count()
            && outputs != components
        {
            return Err(malformed(format!(
                "shading function produces {outputs} outputs, color space has {components} components"
            )));
        }
        let options = self.shading_options(dict, components)?;
        match kind {
            1 => {
                let matrix = self.optional_matrix(dict)?;
                Ok(ShadingModel::Function(FunctionShading {
                    color_space,
                    function,
                    matrix,
                    options,
                }))
            }
            2 => {
                let coords = self.required_real_array(dict, "Coords", 4)?;
                let coords: [f64; 4] = [coords[0], coords[1], coords[2], coords[3]];
                Ok(ShadingModel::Axial(AxialShading {
                    domain: self.shading_domain(dict)?,
                    extend: self.shading_extend(dict)?,
                    color_space,
                    function,
                    coords,
                    options,
                }))
            }
            _ => {
                let coords = self.required_real_array(dict, "Coords", 6)?;
                if coords[2] < 0.0 || coords[5] < 0.0 {
                    return Err(malformed("radial shading radii must be non-negative"));
                }
                let coords: [f64; 6] = [
                    coords[0], coords[1], coords[2], coords[3], coords[4], coords[5],
                ];
                Ok(ShadingModel::Radial(RadialShading {
                    domain: self.shading_domain(dict)?,
                    extend: self.shading_extend(dict)?,
                    color_space,
                    function,
                    coords,
                    options,
                }))
            }
        }
    }

    fn parse_mesh_shading(&self, kind: i64, dict: &Dict, data: &[u8]) -> Result<ShadingModel> {
        let color_space = self.shading_color_space(dict)?;
        let components = color_space
            .component_count()
            .ok_or_else(|| unsupported("shading color space has no fixed component count"))?;
        let indexed = matches!(color_space, ColorSpaceModel::Indexed(_));
        let function = match dict.get("Function") {
            None | Some(Value::Null) => None,
            Some(_) if indexed => {
                return Err(malformed(
                    "mesh /Function must not be combined with an Indexed color space",
                ));
            }
            Some(value) => Some(self.mesh_function(value, components)?),
        };
        let per_record = if function.is_some() { 1 } else { components };
        let bits_per_coordinate =
            self.bit_depth(dict, "BitsPerCoordinate", &[1, 2, 4, 8, 12, 16, 24, 32])?;
        let bits_per_components =
            self.bit_depth(dict, "BitsPerComponent", &[1, 2, 4, 8, 12, 16])?;
        let has_flag = kind == 4 || kind == 6 || kind == 7;
        let bits_per_flag = if has_flag {
            self.bit_depth(dict, "BitsPerFlag", &[2, 4, 8])?
        } else {
            0
        };
        let decode = match self.decode_pairs(dict, 2 + per_record)? {
            Some(pairs) => pairs,
            None if kind == 4 || kind == 5 => {
                return Err(malformed("shading types 4 and 5 require /Decode"));
            }
            None => vec![(0.0, 1.0); 2 + per_record],
        };
        let record_bits =
            bits_per_flag + 2 * bits_per_coordinate + per_record * bits_per_components;
        let total_bits = data.len() * 8;
        if total_bits / record_bits > MAX_MESH_RECORDS {
            return Err(malformed("mesh vertex data exceeds the record limit"));
        }
        let vertices = decode_mesh(
            data,
            record_bits,
            bits_per_flag,
            bits_per_coordinate,
            bits_per_components,
            &decode,
        )?;
        match kind {
            4 => {
                if vertices.len() % 3 != 0 {
                    return Err(malformed(
                        "type 4 mesh vertex records must come in groups of 3",
                    ));
                }
                Ok(ShadingModel::FreeFormMesh(FreeFormMeshShading {
                    color_space,
                    function,
                    bits_per_coordinate,
                    bits_per_components,
                    bits_per_flag,
                    decode,
                    vertices,
                    options: self.shading_options(dict, components)?,
                }))
            }
            5 => {
                let vertices_per_row = dict
                    .get("VerticesPerRow")
                    .and_then(Value::as_integer)
                    .ok_or_else(|| malformed("missing required key /VerticesPerRow"))?;
                if vertices_per_row < 2 {
                    return Err(malformed("/VerticesPerRow must be at least 2"));
                }
                let per_row = vertices_per_row as usize;
                if vertices.len() % per_row != 0 || vertices.is_empty() {
                    return Err(malformed(
                        "type 5 mesh vertex records must fill complete rows",
                    ));
                }
                Ok(ShadingModel::LatticeMesh(LatticeMeshShading {
                    color_space,
                    function,
                    bits_per_coordinate,
                    bits_per_components,
                    vertices_per_row: per_row,
                    decode,
                    vertices,
                    options: self.shading_options(dict, components)?,
                }))
            }
            6 => {
                let patches = group_patches(vertices, 12)?;
                Ok(ShadingModel::CoonsPatch(CoonsPatchShading {
                    color_space,
                    function,
                    bits_per_coordinate,
                    bits_per_components,
                    bits_per_flag,
                    decode,
                    patches,
                    options: self.shading_options(dict, components)?,
                }))
            }
            _ => {
                let patches = group_tensor_patches(vertices)?;
                Ok(ShadingModel::TensorProductPatch(
                    TensorProductPatchShading {
                        color_space,
                        function,
                        bits_per_coordinate,
                        bits_per_components,
                        bits_per_flag,
                        decode,
                        patches,
                        options: self.shading_options(dict, components)?,
                    },
                ))
            }
        }
    }
}
