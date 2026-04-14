pub(crate) fn apply_api_key(url: &str, api_key: &str) -> String {
    if api_key.is_empty() || url.contains("api_key=") {
        return url.to_string();
    }
    let sep = if url.contains('?') { "&" } else { "?" };
    format!("{url}{sep}api_key={api_key}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_key() {
        let url = apply_api_key("http://fra.rpc.orbitflare.com", "ORBIT-123");
        assert_eq!(url, "http://fra.rpc.orbitflare.com?api_key=ORBIT-123");
    }

    #[test]
    fn appends_with_existing_query() {
        let url = apply_api_key("http://fra.rpc.orbitflare.com?foo=bar", "ORBIT-123");
        assert_eq!(url, "http://fra.rpc.orbitflare.com?foo=bar&api_key=ORBIT-123");
    }

    #[test]
    fn skips_if_already_present() {
        let url = "http://fra.rpc.orbitflare.com?api_key=ORBIT-ABC";
        assert_eq!(apply_api_key(url, "ORBIT-123"), url);
    }

    #[test]
    fn skips_if_empty_key() {
        let url = "http://fra.rpc.orbitflare.com";
        assert_eq!(apply_api_key(url, ""), url);
    }
}
