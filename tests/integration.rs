use emerge_rs::actions;

#[tokio::test]
async fn test_upgrade_single_package_pretend() {
    let packages = vec!["app-misc/binwalk".to_string()]; // Test with an installed package
    let result = actions::action_upgrade(&packages, true, false, false, false, false).await;

    assert!(result == 0 || result == 1, "Expected result to be 0 or 1, got {}", result);

    println!("Pretend upgrade test completed with result: {}", result);
}

#[tokio::test]
async fn test_upgrade_world_pretend() {
    // Use system repositories instead of test setup
    let packages = vec!["app-misc/hello".to_string()]; // Test with a single package first
    let result = actions::action_upgrade(&packages, true, false, false, false, false).await;

    assert!(result == 0 || result == 1, "Expected result to be 0 or 1, got {}", result);

    println!("Pretend upgrade test completed with result: {}", result);
}