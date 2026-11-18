# Two render tiers: a WebGL2 floor and a full native tier

The real-time viewport is written once, within the WebGL2 / OpenGL ES 3.0 feature level, so it runs in the browser, on weak GPUs and on phones. On top of it, a full tier switches on when the GPU offers compute shaders, storage buffers and large enough limits through a native API (Vulkan, Metal, or DX12 on Windows); a GPU that only reaches the floor stays on the base tier whatever API it speaks. Only the accumulating Render mode (bounce light, soft shadows, hardware ray tracing where available) is full-tier only, so the path tracer is written once; the viewport differs between tiers only in default quality.

## Considered options

- **One path at the WebGL2 level everywhere**: simplest, but bounce light ("reflex" in academic drawing) would stay an approximation even on strong desktops.
- **A separate full-quality viewport for native GPUs**: best real-time picture, but every viewport feature would be written twice.
