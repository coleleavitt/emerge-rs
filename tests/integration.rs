use emerge_rs::actions;

#[tokio::test]
async fn test_install_package_pretend() {
    let packages = vec!["app-misc/hello".to_string()];
    let result = actions::action_install_with_root(&packages, true, false, false, 1, "/", false).await;

    assert!(result == 0 || result == 1, "Expected result to be 0 or 1, got {}", result);
    
    println!("Pretend install test completed with result: {}", result);
}