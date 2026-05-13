use anyhow::Result;

pub trait TestReviewer {
    async fn run(&self) -> Result<()>;
}

pub struct TestImpl;
impl TestReviewer for TestImpl {
    async fn run(&self) -> Result<()> { Ok(()) }
}

#[allow(dead_code)]
fn test_dyn() {
    let _r: Box<dyn TestReviewer> = Box::new(TestImpl);
}
