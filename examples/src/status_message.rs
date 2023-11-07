use serde_json::json;

const STATUS_MSG_WASM_FILEPATH: &str = "./examples/res/status_message.wasm";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let wasm = std::fs::read(STATUS_MSG_WASM_FILEPATH)?;
    let contract = worker.root_account()?.deploy(&wasm).await?.into_result()?;

    let outcome = contract
        .call("set_status")
        .args_json(json!({
            "message": "hello_world",
        }))
        .transact()
        .await?;
    println!("set_status: {:?}", outcome);

    let result: String = contract
        .view("get_status")
        .args_json(json!({
            "account_id": contract.id(),
        }))
        .await?
        .json()?;

    println!("status: {:?}", result);

    Ok(())
}
