//! Error handling and error type tests

#[cfg(test)]
mod tests {
    use crate::errors::MyError;
    use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
    use std::error::Error;

    #[test]
    fn argument_parse_error_display() {
        let error = MyError::ArgumentParseError("invalid:format".to_string());
        let display_msg = format!("{}", error);
        assert_eq!(display_msg, "unable to parse argument invalid:format");
    }

    #[test]
    fn argument_parse_error_debug() {
        let error = MyError::ArgumentParseError("test".to_string());
        let debug_msg = format!("{:?}", error);
        assert!(debug_msg.contains("ArgumentParseError"));
        assert!(debug_msg.contains("test"));
    }

    #[test]
    fn missing_named_port_error_display() {
        let error = MyError::MissingNamedPort("http".to_string(), "web-service".to_string());
        let display_msg = format!("{}", error);
        assert_eq!(
            display_msg,
            "unable to find named port http on service web-service"
        );
    }

    #[test]
    fn missing_named_port_error_debug() {
        let error = MyError::MissingNamedPort("grpc".to_string(), "api-service".to_string());
        let debug_msg = format!("{:?}", error);
        assert!(debug_msg.contains("MissingNamedPort"));
        assert!(debug_msg.contains("grpc"));
        assert!(debug_msg.contains("api-service"));
    }

    #[test]
    fn service_not_found_error_display() {
        let error = MyError::ServiceNotFound("nonexistent-service".to_string());
        let display_msg = format!("{}", error);
        assert_eq!(
            display_msg,
            "service nonexistent-service not found or invalid"
        );
    }

    #[test]
    fn service_not_found_error_debug() {
        let error = MyError::ServiceNotFound("missing-service".to_string());
        let debug_msg = format!("{:?}", error);
        assert!(debug_msg.contains("ServiceNotFound"));
        assert!(debug_msg.contains("missing-service"));
    }

    #[test]
    fn service_missing_selectors_error_display() {
        let error = MyError::ServiceMissingSelectors("headless-service".to_string());
        let display_msg = format!("{}", error);
        assert_eq!(
            display_msg,
            "service headless-service not compatiable as it is is missing selectors"
        );
    }

    #[test]
    fn service_missing_selectors_error_debug() {
        let error = MyError::ServiceMissingSelectors("external-service".to_string());
        let debug_msg = format!("{:?}", error);
        assert!(debug_msg.contains("ServiceMissingSelectors"));
        assert!(debug_msg.contains("external-service"));
    }

    #[test]
    fn matching_ready_pod_not_found_error_display() {
        let error = MyError::MatchingReadyPodNotFound();
        let display_msg = format!("{}", error);
        assert_eq!(display_msg, "no matching ready pods");
    }

    #[test]
    fn matching_ready_pod_not_found_error_debug() {
        let error = MyError::MatchingReadyPodNotFound();
        let debug_msg = format!("{:?}", error);
        assert!(debug_msg.contains("MatchingReadyPodNotFound"));
    }

    #[test]
    fn could_not_find_port_error_with_numeric_port() {
        let port = IntOrString::Int(65536);
        let error = MyError::CouldNotFindPort(port.clone());
        let display_msg = format!("{}", error);
        assert!(display_msg.contains("service is referencing"));
        assert!(display_msg.contains("65536"));
        assert!(display_msg.contains("but this does not exist on the pod"));
    }

    #[test]
    fn could_not_find_port_error_with_named_port() {
        let port = IntOrString::String("nonexistent".to_string());
        let error = MyError::CouldNotFindPort(port.clone());
        let display_msg = format!("{}", error);
        assert!(display_msg.contains("service is referencing"));
        assert!(display_msg.contains("nonexistent"));
        assert!(display_msg.contains("but this does not exist on the pod"));
    }

    #[test]
    fn could_not_find_port_error_debug() {
        let port = IntOrString::String("debug-port".to_string());
        let error = MyError::CouldNotFindPort(port);
        let debug_msg = format!("{:?}", error);
        assert!(debug_msg.contains("CouldNotFindPort"));
        assert!(debug_msg.contains("debug-port"));
    }

    #[test]
    fn error_trait_implementation() {
        let error = MyError::ArgumentParseError("test".to_string());

        // Test that our error implements the Error trait
        let _error_trait: &dyn Error = &error;

        // Test source (should be None for our simple errors)
        assert!(error.source().is_none());
    }

    #[test]
    fn error_equality_and_pattern_matching() {
        let error1 = MyError::ArgumentParseError("test".to_string());
        let error2 = MyError::ServiceNotFound("service".to_string());
        let error3 = MyError::MatchingReadyPodNotFound();

        // Test pattern matching works correctly
        match error1 {
            MyError::ArgumentParseError(msg) => assert_eq!(msg, "test"),
            _ => panic!("Pattern matching failed"),
        }

        match error2 {
            MyError::ServiceNotFound(service) => assert_eq!(service, "service"),
            _ => panic!("Pattern matching failed"),
        }

        match error3 {
            MyError::MatchingReadyPodNotFound() => {}
            _ => panic!("Pattern matching failed"),
        }
    }

    #[test]
    fn error_conversion_to_anyhow() {
        let error = MyError::ArgumentParseError("test".to_string());
        let anyhow_error: anyhow::Error = error.into();

        let error_msg = format!("{}", anyhow_error);
        assert_eq!(error_msg, "unable to parse argument test");
    }

    #[test]
    fn all_error_variants_have_unique_messages() {
        let errors = vec![
            MyError::ArgumentParseError("test".to_string()),
            MyError::MissingNamedPort("port".to_string(), "service".to_string()),
            MyError::ServiceNotFound("service".to_string()),
            MyError::ServiceMissingSelectors("service".to_string()),
            MyError::MatchingReadyPodNotFound(),
            MyError::CouldNotFindPort(IntOrString::Int(8080)),
        ];

        let messages: Vec<String> = errors.iter().map(|e| format!("{}", e)).collect();

        // Verify each error has a distinct message pattern
        assert!(messages[0].contains("unable to parse argument"));
        assert!(messages[1].contains("unable to find named port"));
        assert!(messages[2].contains("not found or invalid"));
        assert!(messages[3].contains("missing selectors"));
        assert!(messages[4].contains("no matching ready pods"));
        assert!(messages[5].contains("service is referencing"));

        // Ensure no two messages are identical (even with different parameters)
        for (i, msg1) in messages.iter().enumerate() {
            for (j, msg2) in messages.iter().enumerate() {
                if i != j {
                    // Check that the error type indicators are different
                    let words1: Vec<&str> = msg1.split_whitespace().collect();
                    let words2: Vec<&str> = msg2.split_whitespace().collect();

                    // Should have different key identifying words
                    assert!(
                        words1.get(0..3) != words2.get(0..3),
                        "Messages too similar: '{}' vs '{}'",
                        msg1,
                        msg2
                    );
                }
            }
        }
    }

    #[test]
    fn error_with_special_characters_in_service_names() {
        let error = MyError::ServiceNotFound("my-service_v2.test-ns".to_string());
        let display_msg = format!("{}", error);
        assert!(display_msg.contains("my-service_v2.test-ns"));
    }

    #[test]
    fn error_with_unicode_characters() {
        let error = MyError::ArgumentParseError("测试-service:端口".to_string());
        let display_msg = format!("{}", error);
        assert!(display_msg.contains("测试-service:端口"));
    }

    #[test]
    fn error_with_empty_strings() {
        let error1 = MyError::ArgumentParseError("".to_string());
        let error2 = MyError::ServiceNotFound("".to_string());
        let error3 = MyError::MissingNamedPort("".to_string(), "".to_string());

        // Should not panic with empty strings
        let _msg1 = format!("{}", error1);
        let _msg2 = format!("{}", error2);
        let _msg3 = format!("{}", error3);
    }

    #[test]
    fn error_with_very_long_strings() {
        let long_string = "a".repeat(1000);
        let error = MyError::ArgumentParseError(long_string.clone());
        let display_msg = format!("{}", error);
        assert!(display_msg.contains(&long_string));
    }

    #[test]
    fn could_not_find_port_with_zero_port() {
        let port = IntOrString::Int(0);
        let error = MyError::CouldNotFindPort(port);
        let display_msg = format!("{}", error);
        assert!(display_msg.contains("0"));
    }

    #[test]
    fn could_not_find_port_with_max_port() {
        let port = IntOrString::Int(65535);
        let error = MyError::CouldNotFindPort(port);
        let display_msg = format!("{}", error);
        assert!(display_msg.contains("65535"));
    }

    #[test]
    fn could_not_find_port_with_negative_port() {
        let port = IntOrString::Int(-1);
        let error = MyError::CouldNotFindPort(port);
        let display_msg = format!("{}", error);
        assert!(display_msg.contains("-1"));
    }
}
