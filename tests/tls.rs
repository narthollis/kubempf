// Regression test for: https://github.com/narthollis/kubempf/pull/116
//
// kube 3.x / rustls 0.23+ require a TLS crypto backend (ring or aws-lc-rs) to
// be explicitly enabled as a feature. With `default-features = false` and only
// `rustls-tls` selected, neither backend is pulled in and `Client::try_from`
// panics before any network I/O:
//
//   thread 'main' panicked at rustls-0.23.x/src/crypto/mod.rs:
//   "Could not automatically determine the process-level CryptoProvider …"

#[tokio::test]
async fn tls_crypto_provider_is_available() {
    let config = kube::Config::new("https://localhost:1".parse().unwrap());

    // Triggers rustls TLS stack initialisation. Panics on missing CryptoProvider;
    // succeeds (even without a reachable server) when ring/aws-lc-rs is present.
    kube::Client::try_from(config).expect("kube Client should initialise without panicking");
}
