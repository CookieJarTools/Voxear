# AMD FSR 3.1's upscaler, as Voxear builds it

`amd/` holds the shaders of AMD FSR 3.1's temporal upscaler, from AMD's FidelityFX SDK
(<https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK>, SDK 2.4.0, commit
7539ae8b6568ad29ed78a753404351daf3b5cca7, upscaler 3.1.6), at the same paths as in the SDK's
`Kits/FidelityFX` folder. Every file here is on the list of files `amd/LICENSE.md` gives the MIT licence;
the licence and AMD's copyright notice stay with them.

AMD ships FSR's host library and its compiled shaders for Windows only. Voxear compiles the passes it uses
to SPIR-V with slangc as it builds (`build.rs`), with one set of options (HDR colour, motion vectors at
the size the scene is drawn, reverse-Z depth), and runs them from its own host code (`src/fsr3.rs`).

## What is changed from AMD's files

slangc keeps a `for` loop's variable in scope after the loop, as older HLSL did, so two loops of one
function with the same variable clash. The second of each pair is renamed; nothing else is changed:

- `upscalers/fsr3/include/gpu/fsr3upscaler/ffx_fsr3upscaler_upsample.h`: `row` to `rowSecond`, in the
  second loop over the 3×3 samples.
- `upscalers/fsr3/include/gpu/fsr3upscaler/ffx_fsr3upscaler_prepare_reactivity.h`: `iSampleIndex` to
  `iSampleIndexSecond`, in the second loop over the samples.
