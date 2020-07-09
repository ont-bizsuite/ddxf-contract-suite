#![cfg_attr(not(feature = "mock"), no_std)]
#![feature(proc_macro_hygiene)]
extern crate alloc;
extern crate common;
extern crate ontio_std as ostd;
use alloc::collections::btree_map::BTreeMap;
use common::*;
use ostd::abi::{EventBuilder, Sink, Source};
use ostd::database;
use ostd::prelude::*;
use ostd::runtime;
use ostd::types::{Address, U128};
mod basic;
use basic::*;
use common::CONTRACT_COMMON;
use ostd::runtime::check_witness;

mod oep8;

#[cfg(test)]
mod test;

#[cfg(test)]
mod oep8_test;

const KEY_DTOKEN: &[u8] = b"01";
const KEY_DDXF_CONTRACT: &[u8] = b"02";
const KEY_ADMIN: &[u8] = b"03";
const PRE_ID: &[u8] = b"04";
const KEY_TT_ID: &[u8] = b"05";
const PRE_TT: &[u8] = b"06";
const PRE_AUTHORIZED: &[u8] = b"07";
const PRE_TOKEN_ID: &[u8] = b"08";

/// set marketplace contract address, need admin signature
///
/// only marketplace contract has the right to invoke some method
fn set_mp_contract(new_addr: &Address) -> bool {
    let admin = get_admin();
    assert!(check_witness(&admin));
    database::put(KEY_DDXF_CONTRACT, new_addr);
    true
}

/// query marketplace contract address
fn get_mp_contract() -> Address {
    database::get(KEY_DDXF_CONTRACT).unwrap()
}

/// update admin address
///
/// need old admin signature
fn update_admin(new_admin: &Address) -> bool {
    let old_admin = get_admin();
    assert!(check_witness(&old_admin));
    database::put(KEY_ADMIN, new_admin);
    true
}

/// query admin address
fn get_admin() -> Address {
    database::get::<_, Address>(KEY_ADMIN).unwrap_or(*CONTRACT_COMMON.admin())
}

/// generate dtoken
///
/// when the user calls buy dtoken in marketplace contract, marketplace contract will call the generate_dtoken method of the contract to generate dtoken for the buyer
///
/// `account` is the buyer address
///
/// `resource_id` used to mark the only commodity in the chain
///
/// `token_template_bytes` used to mark the only token
///
/// `n` represents the number of generate tokens
pub fn generate_dtoken(account: &Address, templates_bytes: &[u8], n: U128) -> bool {
    let mut source = Source::new(templates_bytes);
    let templates: Vec<TokenTemplate> = source.read().unwrap();
    check_caller();
    assert!(runtime::check_witness(account));
    for token_template in templates.iter() {
        let token_id = oep8::generate_token(b"", b"", n, account);
        let mut caa = get_count_and_agent(account, token_id.as_slice());
        caa.count += n as u32;
        database::put(
            utils::gen_key(token_template.to_bytes().as_slice()),
            token_id.as_slice(),
        );
        update_count(account, token_id.as_slice(), caa.clone());
    }
    EventBuilder::new()
        .string("generateDToken")
        .address(account)
        .number(n)
        .notify();
    true
}

pub fn create_token_template(creator: &Address, tt_bs: &[u8]) -> bool {
    assert!(check_witness(creator));
    let tt_id = get_next_tt_id();
    let tt_id_str = tt_id.to_string();
    database::put(
        get_key(PRE_TT, tt_id_str.as_bytes()),
        TokenTemplateInfo::new(creator.clone(), tt_bs.to_vec()),
    );
    update_next_tt_id(tt_id + 1);
    EventBuilder::new()
        .string("create_token_template")
        .address(creator)
        .bytearray(tt_bs)
        .bytearray(tt_id_str.as_bytes())
        .notify();
    true
}

pub fn authorize_token_template(token_template_id: &[u8], authorized_addr: &Address) -> bool {
    let tt_info = database::get::<_, TokenTemplateInfo>(get_key(PRE_TT, token_template_id))
        .expect("not existed token template");
    assert!(check_witness(&tt_info.creator));
    let mut addrs = get_authorized_addr(token_template_id);
    let index = addrs.iter().position(|x| x == authorized_addr);
    if index.is_none() {
        addrs.push(*authorized_addr);
        let key = get_key(PRE_AUTHORIZED, token_template_id);
        database::put(key.as_slice(), addrs);
    }
    true
}

fn get_authorized_addr(token_template_id: &[u8]) -> Vec<Address> {
    let key = get_key(PRE_AUTHORIZED, token_template_id);
    database::get::<_, Vec<Address>>(key.as_slice()).unwrap_or(vec![])
}

fn is_valid_addr(acc: &Address, token_template_id: &[u8]) -> bool {
    let tt_info = database::get::<_, TokenTemplateInfo>(get_key(PRE_TT, token_template_id))
        .expect("not existed token template");
    if &tt_info.creator == acc {
        return true;
    } else {
        let addrs = get_authorized_addr(token_template_id);
        let index = addrs.iter().position(|x| x == acc);
        if index.is_some() {
            return true;
        }
    }
    false
}

pub fn generate_token(acc: &Address, token_template_id: &[u8], n: U128) -> bool {
    assert!(check_caller() || is_valid_addr(acc, token_template_id));
    assert!(check_witness(acc));
    let token_id = oep8::generate_token(b"oep8", b"DToken", n, acc);
    let key = get_key(PRE_TOKEN_ID, token_id.as_slice());
    database::put(key.as_slice(), token_id.as_slice());
    EventBuilder::new()
        .string("generate_token")
        .address(acc)
        .bytearray(token_template_id)
        .number(n)
        .bytearray(token_id.as_slice())
        .notify();
    true
}

fn get_key(pre: &[u8], post: &[u8]) -> Vec<u8> {
    [pre, post].concat()
}
fn get_next_tt_id() -> U128 {
    database::get::<_, U128>(KEY_TT_ID).unwrap_or(0)
}
fn update_next_tt_id(new_id: U128) {
    database::put(KEY_TT_ID, new_id)
}

/// use token, the buyer of the token has the right to consume the token
///
/// `account` is the buyer address
///
/// `resource_id` used to mark the only commodity in the chain
///
/// `token_template_bytes` used to mark the only token
///
/// `n` represents the number of consuming token
pub fn use_token(account: &Address, token_template_bytes: &[u8], n: U128) -> bool {
    assert!(check_witness(account));
    let mut caa = get_count_and_agent(account, token_template_bytes);
    assert!(caa.count >= n as u32);
    caa.count -= n as u32;
    let key = utils::generate_dtoken_key(account, token_template_bytes);
    if caa.count == 0 {
        database::delete(key);
    } else {
        database::put(key, caa);
    }
    EventBuilder::new()
        .string("useToken")
        .address(account)
        .number(n)
        .notify();
    true
}

fn delete_token(account: &Address, token_template_bytes: &[u8]) -> bool {
    assert!(check_witness(account) || check_witness(CONTRACT_COMMON.admin()));
    let caa = get_count_and_agent(account, token_template_bytes);
    assert_eq!(caa.count, 0);
    database::delete(utils::generate_dtoken_key(account, token_template_bytes));
    true
}

/// use token by agent, the agent of the token has the right to invoke this method
///
/// `account` is the buyer address
///
/// `agent` is the authorized address
///
/// `resource_id` used to mark the only commodity in the chain
///
/// `token_template_bytes` used to mark the only token
///
/// `n` represents the number of consuming token
pub fn use_token_by_agent(
    account: &Address,
    agent: &Address,
    token_template_bytes: &[u8],
    n: U128,
) -> bool {
    assert!(check_witness(agent));
    let mut caa = get_count_and_agent(account, token_template_bytes);
    assert!(caa.count >= n as u32);
    let agent_count = caa.agents.get_mut(agent).unwrap();
    assert!(*agent_count >= n as u32);
    if caa.count == n as u32 && *agent_count == n as u32 {
        database::delete(utils::generate_dtoken_key(account, token_template_bytes));
    } else {
        caa.count -= n as u32;
        *agent_count -= n as u32;
        update_count(account, token_template_bytes, caa);
    }
    EventBuilder::new()
        .string("useTokenByAgent")
        .address(account)
        .number(n)
        .notify();
    true
}

pub fn transfer_dtoken(
    from_account: &Address,
    to_account: &Address,
    templates_bytes: &[u8],
    n: U128,
) -> bool {
    assert!(check_witness(from_account));
    let mut source = Source::new(templates_bytes);
    let templates: Vec<TokenTemplate> = source.read().unwrap();
    for token_template in templates.iter() {
        let template_bytes = token_template.to_bytes();
        let mut from_caa = get_count_and_agent(from_account, &template_bytes);
        assert!(from_caa.count >= n as u32);
        from_caa.count -= n as u32;
        update_count(from_account, &template_bytes, from_caa);
        let mut to_caa = get_count_and_agent(to_account, &template_bytes);
        to_caa.count += n as u32;
        update_count(to_account, &template_bytes, to_caa);
    }
    true
}

/// set agents, this method will set agents more than one TokeTemplate
///
/// `account` is the buyer address
///
/// `resource_id` used to mark the only commodity in the chain
///
/// `agents` is the array of address who will be authorized agents
///
/// `n` represents the number of authorized token
///
/// `token_template_bytes` is array of TokenTemplate
pub fn set_agents(
    account: &Address,
    agents: Vec<Address>,
    n: U128,
    token_templates_bytes: &[u8],
) -> bool {
    assert!(check_witness(account));
    let mut source = Source::new(token_templates_bytes);
    let token_templates: Vec<TokenTemplate> = source.read().unwrap();
    assert!(check_witness(account));
    for token_template in token_templates.iter() {
        assert!(set_token_agents(
            account,
            &token_template.to_bytes(),
            agents.clone(),
            n
        ));
    }
    true
}

/// set token agents
///
/// `account` is the buyer address
///
/// `resource_id` used to mark the only commodity in the chain
///
/// `token_template_bytes` used to mark the only token
///
/// `agents` is the array of address who will be authorized as agents
///
/// `n` represents the number of authorized token
pub fn set_token_agents(
    account: &Address,
    token_template_bytes: &[u8],
    agents: Vec<Address>,
    n: U128,
) -> bool {
    assert!(check_witness(account));
    let mut caa = get_count_and_agent(account, token_template_bytes);
    caa.set_token_agents(agents.as_slice(), n);
    update_count(account, token_template_bytes, caa);
    EventBuilder::new()
        .string("setTokenAgents")
        .address(account)
        .number(n)
        .notify();
    true
}

/// add_agents, this method append agents for the all token
///
/// `resource_id` used to mark the only commodity in the chain
///
/// `account` is user address who authorize the other address is agent, need account signature
///
/// `agents` is the array of agent address
///
/// `n` is number of authorizations per agent
pub fn add_agents(
    account: &Address,
    agents: Vec<Address>,
    n: U128,
    token_templates_bytes: &[u8],
) -> bool {
    assert!(check_witness(account));
    let mut source = Source::new(token_templates_bytes);
    let token_templates: Vec<TokenTemplate> = source.read().unwrap();
    for token_template in token_templates.iter() {
        assert!(add_token_agents(
            account,
            &token_template.to_bytes(),
            &agents,
            n
        ));
    }
    true
}

/// add_agents, this method only append agents for the specified token.
///
/// `resource_id` used to mark the only commodity in the chain
///
/// `account` is user address who authorize the other address is agent, need account signature
///
/// `token_template_bytes` used to specified which token to set agents.
///
/// `agents` is the array of agent address
///
/// `n` is number of authorizations per agent
pub fn add_token_agents(
    account: &Address,
    token_template_bytes: &[u8],
    agents: &[Address],
    n: U128,
) -> bool {
    assert!(check_witness(account));
    let mut caa = get_count_and_agent(account, token_template_bytes);
    caa.add_agents(agents, n as u32);
    update_count(account, token_template_bytes, caa);
    EventBuilder::new()
        .string("addTokenAgents")
        .address(account)
        .number(n)
        .notify();
    true
}

/// product owner remove all the agents
///
/// `account` is user address who authorize the other address is agent, need account signature
///
/// `resource_id` used to mark the only commodity in the chain
///
/// `agents` is the array of agent address which will be removed by account
///
/// `token_templates_bytes` the serialization result is array of TokenTemplate
pub fn remove_agents(
    account: &Address,
    agents: Vec<Address>,
    token_templates_bytes: &[u8],
) -> bool {
    assert!(check_witness(account));
    let mut source = Source::new(token_templates_bytes);
    let token_templates: Vec<TokenTemplate> = source.read().unwrap();
    for token_template in token_templates.iter() {
        assert!(remove_token_agents(
            account,
            &token_template.to_bytes(),
            agents.as_slice()
        ));
    }
    true
}

/// product owner remove the agents of specified token
///
/// `account` is user address who authorize the other address is agent, need account signature
///
/// `resource_id` used to mark the only commodity in the chain
///
/// `token_template_bytes` is the serialization result of
///
/// `agents` is the array of agent address which will be removed by account
pub fn remove_token_agents(
    account: &Address,
    token_template_bytes: &[u8],
    agents: &[Address],
) -> bool {
    assert!(check_witness(account));
    let mut caa = get_count_and_agent(account, token_template_bytes);
    caa.remove_agents(agents);
    update_count(account, token_template_bytes, caa);
    EventBuilder::new()
        .string("removeTokenAgents")
        .address(account)
        .notify();
    true
}

fn check_caller() -> bool {
    let caller = runtime::caller();
    let ddxf = get_mp_contract();
    assert!(caller == ddxf);
    true
}

fn get_count_and_agent(account: &Address, token_template_bytes: &[u8]) -> CountAndAgent {
    let key = utils::generate_dtoken_key(account, token_template_bytes);
    database::get::<_, CountAndAgent>(&key).unwrap_or(CountAndAgent::new(account.clone()))
}

fn update_count(account: &Address, token_id: &[u8], caa: CountAndAgent) {
    let key = utils::generate_dtoken_key(account, token_id);
    database::put(key, caa);
}

#[no_mangle]
pub fn invoke() {
    let input = runtime::input();
    let mut source = Source::new(&input);
    let action: &[u8] = source.read().unwrap();
    let mut sink = Sink::new(12);
    match action {
        b"updateAdmin" => {
            let new_admin = source.read().unwrap();
            sink.write(update_admin(&new_admin));
        }
        b"getAdmin" => {
            sink.write(get_admin());
        }
        b"setDdxfContract" => {
            let new_addr = source.read().unwrap();
            sink.write(set_mp_contract(new_addr));
        }
        b"getDdxfContract" => {
            sink.write(get_mp_contract());
        }
        b"migrate" => {
            let (code, vm_type, name, version, author, email, desc) = source.read().unwrap();
            sink.write(CONTRACT_COMMON.migrate(code, vm_type, name, version, author, email, desc));
        }
        b"generateDToken" => {
            let (account, templates, n) = source.read().unwrap();
            sink.write(generate_dtoken(account, templates, n));
        }
        b"deleteToken" => {
            let (account, templates) = source.read().unwrap();
            sink.write(delete_token(account, templates));
        }
        b"getCountAndAgent" => {
            let (account, token_template) = source.read().unwrap();
            sink.write(get_count_and_agent(account, token_template));
        }
        b"useToken" => {
            let (account, token_template, n) = source.read().unwrap();
            sink.write(use_token(account, token_template, n));
        }
        b"useTokenByAgent" => {
            let (account, agent, token_template, n) = source.read().unwrap();
            sink.write(use_token_by_agent(account, agent, token_template, n));
        }
        b"transferDToken" => {
            let (from_account, to_account, templates_bytes, n) = source.read().unwrap();
            sink.write(transfer_dtoken(
                from_account,
                to_account,
                templates_bytes,
                n,
            ));
        }
        b"setAgents" => {
            let (account, agents, n, token_templates) = source.read().unwrap();
            sink.write(set_agents(account, agents, n, token_templates));
        }
        b"setTokenAgents" => {
            let (account, token_template, agents, n) = source.read().unwrap();
            sink.write(set_token_agents(account, token_template, agents, n));
        }
        b"addAgents" => {
            let (account, agents, n, token_templates) = source.read().unwrap();
            sink.write(add_agents(account, agents, n, token_templates));
        }
        b"addTokenAgents" => {
            let (account, token_template, agents, n): (&Address, &[u8], Vec<Address>, U128) =
                source.read().unwrap();
            sink.write(add_token_agents(
                account,
                token_template,
                agents.as_slice(),
                n,
            ));
        }
        b"removeAgents" => {
            let (account, agents, token_templates) = source.read().unwrap();
            sink.write(remove_agents(account, agents, token_templates));
        }
        b"removeTokenAgents" => {
            let (account, token_template, agents): (&Address, &[u8], Vec<Address>) =
                source.read().unwrap();
            sink.write(remove_token_agents(
                account,
                token_template,
                agents.as_slice(),
            ));
        }
        _ => {
            let method = str::from_utf8(action).ok().unwrap();
            panic!("dtoken contract, not support method:{}", method)
        }
    }
    runtime::ret(sink.bytes());
}

mod utils {
    use super::*;
    use alloc::vec::Vec;
    pub fn generate_dtoken_key(account: &Address, token_id: &[u8]) -> Vec<u8> {
        [KEY_DTOKEN, account.as_ref(), token_id].concat()
    }
    pub fn gen_key(token_template_bytes: &[u8]) -> Vec<u8> {
        [PRE_ID, token_template_bytes].concat()
    }
}
