//! Root signatures of our own shaders, made here rather than taken from the
//! compiled shader. Under Wine `D3DCompile` is vkd3d-shader's compiler,
//! which ignores `[RootSignature]` and embeds none, so `CreateRootSignature`
//! on the shader's bytes fails there. The flags are those a version 1.1 root
//! signature string gets by default, so the result is what the string in
//! the shader describes.

use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D12::*;

/// One root parameter, as a root signature string names it.
#[derive(Clone, Copy)]
pub(crate) enum Param {
    /// `RootConstants(num32BitConstants=count, b<register>)`.
    Constants { register: u32, count: u32 },
    /// `UAV(u<register>)`: a root descriptor.
    Uav(u32),
    /// `DescriptorTable(SRV(t<register>, numDescriptors=count))`.
    SrvTable { register: u32, count: u32, visibility: D3D12_SHADER_VISIBILITY },
    /// `DescriptorTable(UAV(u<register>, numDescriptors=count))`.
    UavTable { register: u32, count: u32 },
}

/// The root signature of `params`, without static samplers.
pub(crate) fn make(device: &ID3D12Device, params: &[Param], what: &str) -> windows::core::Result<ID3D12RootSignature> {
    // The ranges first: the parameters point into them.
    let ranges: Vec<D3D12_DESCRIPTOR_RANGE1> = params
        .iter()
        .map(|p| {
            let (kind, register, count, flags) = match *p {
                Param::SrvTable { register, count, .. } => (
                    D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
                    register,
                    count,
                    D3D12_DESCRIPTOR_RANGE_FLAG_DATA_STATIC_WHILE_SET_AT_EXECUTE,
                ),
                Param::UavTable { register, count } => {
                    (D3D12_DESCRIPTOR_RANGE_TYPE_UAV, register, count, D3D12_DESCRIPTOR_RANGE_FLAG_DATA_VOLATILE)
                }
                _ => (D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 0, 0, D3D12_DESCRIPTOR_RANGE_FLAG_NONE),
            };
            D3D12_DESCRIPTOR_RANGE1 {
                RangeType: kind,
                NumDescriptors: count,
                BaseShaderRegister: register,
                RegisterSpace: 0,
                Flags: flags,
                OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
            }
        })
        .collect();
    let built: Vec<D3D12_ROOT_PARAMETER1> = params
        .iter()
        .zip(&ranges)
        .map(|(p, range)| match *p {
            Param::Constants { register, count } => D3D12_ROOT_PARAMETER1 {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
                Anonymous: D3D12_ROOT_PARAMETER1_0 {
                    Constants: D3D12_ROOT_CONSTANTS { ShaderRegister: register, RegisterSpace: 0, Num32BitValues: count },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            Param::Uav(register) => D3D12_ROOT_PARAMETER1 {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_UAV,
                Anonymous: D3D12_ROOT_PARAMETER1_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR1 {
                        ShaderRegister: register,
                        RegisterSpace: 0,
                        Flags: D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            Param::SrvTable { .. } | Param::UavTable { .. } => D3D12_ROOT_PARAMETER1 {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER1_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE1 { NumDescriptorRanges: 1, pDescriptorRanges: range },
                },
                ShaderVisibility: match *p {
                    Param::SrvTable { visibility, .. } => visibility,
                    _ => D3D12_SHADER_VISIBILITY_ALL,
                },
            },
        })
        .collect();
    let desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
        Version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
            Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                NumParameters: built.len() as u32,
                pParameters: built.as_ptr(),
                NumStaticSamplers: 0,
                pStaticSamplers: std::ptr::null(),
                Flags: D3D12_ROOT_SIGNATURE_FLAG_NONE,
            },
        },
    };
    let mut blob: Option<ID3DBlob> = None;
    let mut error: Option<ID3DBlob> = None;
    // SAFETY: `desc` points at the live `built` and `ranges`; the blobs are
    // read in full before they are released.
    unsafe {
        if let Err(e) = D3D12SerializeVersionedRootSignature(&desc, &mut blob, Some(&mut error)) {
            if let Some(error) = error {
                let text = std::slice::from_raw_parts(error.GetBufferPointer().cast::<u8>(), error.GetBufferSize());
                tracing::warn!("{what} root signature: {}", String::from_utf8_lossy(text));
            }
            return Err(e);
        }
        let blob = blob.ok_or_else(windows::core::Error::empty)?;
        let bytes = std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize());
        device.CreateRootSignature(0, bytes)
    }
}
