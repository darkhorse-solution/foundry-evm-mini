use anyhow::{anyhow, Result};
use ethers::abi::parse_abi;
use ethers::providers::{Middleware, Provider, Ws};
use ethers::types::{BlockNumber, H160};
use ethers_contract::BaseContract;
use revm::{
    db::CacheDB,
    primitives::{ExecutionResult, Output, TransactTo, U256 as rU256},
    EVM,
};
use std::{collections::BTreeSet, str::FromStr, sync::Arc};
use ethers_core::types::U64;
use ethers_core::utils::Units::Gwei;
use ethers_providers::Http;
use foundry_evm_mini::evm::executor::{
    fork::{BlockchainDb, BlockchainDbMeta, SharedBackend},
    inspector::{get_precompiles_for, AccessListTracer},
};

#[tokio::main]
async fn main() -> Result<()> {
    let wss_url = "wss://eth-mainnet.g.alchemy.com/v2/ainc64D6aQZ6i7QwDkYqUkSVj754iwGz";
    // let ws = Ws::connect(wss_url).await.unwrap();

    let provider: Provider<Http> = Provider::<Http>::try_from("https://bsc-dataseed.binance.org/")
        .expect("could not instantiate HTTP Provider");
    let provider: Arc<Provider<Http>> = Arc::new(provider.clone());
    // let provider = Arc::new(Provider::new(ws));

    let block = provider
        .get_block(BlockNumber::Latest)
        .await
        .unwrap()
        .unwrap();

    let shared_backend = SharedBackend::spawn_backend_thread(
        provider.clone(),
        BlockchainDb::new(
            BlockchainDbMeta {
                cfg_env: Default::default(),
                block_env: Default::default(),
                hosts: BTreeSet::from(["".to_string()]),
            },
            None,
        ),
        Some(block.number.unwrap().into()),
    );
    let db = CacheDB::new(shared_backend);

    let mut evm = EVM::new();
    evm.database(db);

    evm.env.cfg.limit_contract_code_size = Some(0x100000);
    evm.env.cfg.disable_block_gas_limit = true;
    evm.env.cfg.disable_base_fee = true;
    evm.env.cfg.chain_id = 56;

    evm.env.block.number = rU256::from(block.number.unwrap().as_u64() + 1);

    let uniswap_v2_factory = BaseContract::from(parse_abi(&[
        "function getPair(address,address) external view returns (address)",
    ])?);

    let factory = H160::from_str("0xca143ce32fe78f1f7019d7d551a6402fc5350c73").unwrap();
    let weth = H160::from_str("0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c").unwrap();
    let usdt = H160::from_str("0x55d398326f99059ff775485246999027b3197955").unwrap();

    let calldata = uniswap_v2_factory.encode("getPair", (weth, usdt))?;

    evm.env.tx.caller = H160::from_str("0x91450cBfADD98984f77A9fC2FE48e4dA0ae0eEa2")
        .unwrap()
        .into();
    evm.env.tx.transact_to = TransactTo::Call(factory.into());
    evm.env.tx.data = calldata.0;
    evm.env.tx.value = rU256::ZERO;
    evm.env.tx.gas_limit = 5000000;
    // evm.env.tx.gas_price = rU256::from(1000000000_u64);
    evm.env.tx.chain_id = Some(56);

    println!("{:?}", evm.env);

    let ref_tx = evm.transact_ref()?;
    let result = ref_tx.result;

    match result {
        ExecutionResult::Success { output, logs, .. } => match output {
            Output::Call(o) => {
                let pair_address: H160 = uniswap_v2_factory.decode_output("getPair", o)?;
                println!("Pair address: {:?}", pair_address);

                for log in logs {
                    println!("{:?}", log);
                }
            }
            _ => {}
        },
        _ => {}
    };

    // get access list example
    let mut access_list_inspector = AccessListTracer::new(
        Default::default(),
        evm.env.tx.caller.into(),
        factory,
        get_precompiles_for(evm.env.cfg.spec_id),
    );
    evm.inspect_ref(&mut access_list_inspector)
        .map_err(|e| anyhow!("[EVM ERROR] access list: {:?}", (e)))?;
    let access_list = access_list_inspector.access_list();
    println!("{:?}", access_list);

    Ok(())
}
