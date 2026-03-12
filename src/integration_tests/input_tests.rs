#[cfg(test)]
mod tests {
    /////////////////////////////////////
    // Input
    /////////////////////////////////////

    // TODO integrate PaddleTest3 ROM suite

    // TODO integrate ruder-0.03 ROM suite

    // TODO integrate spadtest-nes-0.01 ROM suite

    // TODO integrate vaus-test-0.02 ROM suite

    /////////////////////////////////////
    // Allpads harness smoke test
    /////////////////////////////////////

    use crate::integration_tests::allpads_harness::tests::{ControllerConfig, run_allpads};

    #[test]
    fn allpads_harness_smoke_test() {
        let config = ControllerConfig::joypad_port1();
        let result = run_allpads(&config, &[], 60, 0);
        assert_eq!(
            result.captures.len(),
            1,
            "Should capture nametable at final frame"
        );
        assert!(
            !result.captures[0].nametable_text.is_empty(),
            "Nametable text should not be empty after 60 frames"
        );
    }
}
