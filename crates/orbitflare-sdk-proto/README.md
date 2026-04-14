# orbitflare-sdk-proto

Internal crate for [`orbitflare-sdk`](https://crates.io/crates/orbitflare-sdk). Contains the compiled protobuf types for OrbitFlare's gRPC services (Yellowstone Geyser and JetStream).

You should not depend on this crate directly. Use [`orbitflare-sdk`](https://crates.io/crates/orbitflare-sdk) instead:

```bash
cargo add orbitflare-sdk --features grpc
```
