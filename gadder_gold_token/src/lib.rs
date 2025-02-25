#![allow(unexpected_cfgs)]

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_option::COption,
    program_pack::{Pack},
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
    system_instruction,
};
use spl_token::{
    instruction as token_instruction,
    state::{Account as TokenAccount, Mint},
};
use mpl_token_metadata::instructions::{CreateMetadataAccountV3, CreateMetadataAccountV3InstructionArgs};

mod ai_contract;
mod governance_contract;
mod staking_contract;
mod cross_chain_bridge_contract;

pub const ADMIN_PUBKEY: Pubkey = Pubkey::new_from_array([0xAA; 32]);
pub const GOVERNANCE_PUBKEY: Pubkey = Pubkey::new_from_array([0xBB; 32]);
pub const BRIDGE_ADMIN_PUBKEY: Pubkey = Pubkey::new_from_array([0xCC; 32]);

pub struct TokenContract;

impl TokenContract {
    pub fn initialize_token(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let mint_acc = next_account_info(account_info_iter)?;
        let authority_acc = next_account_info(account_info_iter)?;
        let _token_program_acc = next_account_info(account_info_iter)?;

        if !authority_acc.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let decimals = 9u8;
        let mint_data = Mint {
            mint_authority: COption::Some(*authority_acc.key),
            supply: 0,
            decimals,
            is_initialized: true,
            freeze_authority: COption::None,
        };
        let rent = Rent::get()?;
        let space = Mint::LEN;
        let lamports = rent.minimum_balance(space);

        invoke(
            &system_instruction::create_account(
                authority_acc.key,
                mint_acc.key,
                lamports,
                space as u64,
                &spl_token::id(),
            ),
            &[authority_acc.clone(), mint_acc.clone()],
        )?;

        Mint::pack(mint_data, &mut mint_acc.try_borrow_mut_data()?)?;

        let metadata_accounts = accounts[3..].to_vec();
        Self::create_token_metadata(
            program_id,
            &metadata_accounts,
            "Gadder Gold",
            "GGT",
            "http://example.com/metadata",
        )?;
        msg!("Token initialized with metadata!");
        Ok(())
    }

    pub fn transfer_tokens(_program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let source_acc = next_account_info(account_info_iter)?;
        let dest_acc = next_account_info(account_info_iter)?;
        let owner_acc = next_account_info(account_info_iter)?;
        let token_program_acc = next_account_info(account_info_iter)?;
        let delegate_acc = next_account_info(account_info_iter).ok();

        if !owner_acc.is_signer && delegate_acc.map_or(true, |d| !d.is_signer) {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let source_token_acc = TokenAccount::unpack(&source_acc.try_borrow_data()?)?;
        if let Some(delegate) = delegate_acc {
            if source_token_acc.delegate != COption::Some(*delegate.key) || source_token_acc.delegated_amount < amount {
                return Err(ProgramError::InsufficientFunds);
            }
        }

        let ix = token_instruction::transfer(
            token_program_acc.key,
            source_acc.key,
            dest_acc.key,
            owner_acc.key,
            &[],
            amount,
        )?;

        invoke(&ix, &[source_acc.clone(), dest_acc.clone(), owner_acc.clone(), token_program_acc.clone()])?;
        msg!("Transferred {} tokens!", amount);
        Ok(())
    }

    pub fn burn_tokens(_program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let token_account = next_account_info(account_info_iter)?;
        let mint_account = next_account_info(account_info_iter)?;
        let burn_authority = next_account_info(account_info_iter)?;
        let token_program_acc = next_account_info(account_info_iter)?;

        if burn_authority.key != &ADMIN_PUBKEY && burn_authority.key != &GOVERNANCE_PUBKEY {
            msg!("Unauthorized burn attempt!");
            return Err(ProgramError::IllegalOwner);
        }
        if !burn_authority.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let ix = token_instruction::burn(
            token_program_acc.key,
            token_account.key,
            mint_account.key,
            burn_authority.key,
            &[],
            amount,
        )?;

        invoke(
            &ix,
            &[
                token_account.clone(),
                mint_account.clone(),
                burn_authority.clone(),
                token_program_acc.clone(),
            ],
        )?;
        msg!("Burned {} tokens!", amount);
        Ok(())
    }

    fn create_token_metadata(
        _program_id: &Pubkey,
        accounts: &[AccountInfo],
        name: &str,
        symbol: &str,
        uri: &str,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let metadata_pda_acc = next_account_info(account_info_iter)?;
        let mint_acc = next_account_info(account_info_iter)?;
        let mint_authority = next_account_info(account_info_iter)?;
        let payer_acc = next_account_info(account_info_iter)?;
        let update_authority = next_account_info(account_info_iter)?;
        let system_program = next_account_info(account_info_iter)?;
        let rent_sysvar = next_account_info(account_info_iter)?;
        let _token_metadata_program = next_account_info(account_info_iter)?;

        let ix = CreateMetadataAccountV3 {
            metadata: *metadata_pda_acc.key,
            mint: *mint_acc.key,
            mint_authority: *mint_authority.key,
            payer: *payer_acc.key,
            update_authority: (*update_authority.key, true),
            system_program: solana_program::system_program::id(),
            rent: Some(*rent_sysvar.key),
        }.instruction(CreateMetadataAccountV3InstructionArgs {
            data: mpl_token_metadata::types::DataV2 {
                name: name.to_string(),
                symbol: symbol.to_string(),
                uri: uri.to_string(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            is_mutable: true,
            collection_details: None,
        });

        invoke_signed(
            &ix,
            &[
                metadata_pda_acc.clone(),
                mint_acc.clone(),
                mint_authority.clone(),
                payer_acc.clone(),
                update_authority.clone(),
                system_program.clone(),
                rent_sysvar.clone(),
            ],
            &[],
        )?;

        msg!("Created token metadata for mint {}", mint_acc.key);
        Ok(())
    }
}

entrypoint!(process_instruction);

fn process_instruction(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    let (tag, rest) = data.split_at(1);

    match tag[0] {
        0 => TokenContract::initialize_token(program_id, accounts),
        1 => {
            let amount = parse_amount(rest)?;
            TokenContract::transfer_tokens(program_id, accounts, amount)
        }
        2 => {
            let amount = parse_amount(rest)?;
            TokenContract::burn_tokens(program_id, accounts, amount)
        }
        3 => {
            let amount = parse_amount(rest)?;
            let lock_period_in_days = parse_amount(&rest[8..])?;
            let mut staking_contract = staking_contract::StakingContract::new();
            staking_contract.stake_tokens(program_id, accounts, amount, lock_period_in_days)
        }
        4 => {
            let amount = parse_amount(rest)?;
            let mut staking_contract = staking_contract::StakingContract::new();
            staking_contract.unstake_tokens(program_id, accounts, amount)
        }
        5 => {
            let description = String::from_utf8_lossy(rest);
            governance_contract::GovernanceContract::create_proposal(program_id, accounts, &description)
        }
        6 => {
            let proposal_id = parse_amount(rest)?;
            governance_contract::GovernanceContract::execute_proposal(program_id, accounts, proposal_id)
        }
        7 => {
            let proposal_id = parse_amount(rest)?;
            let vote = rest.get(8).cloned().unwrap_or(0) == 1;
            governance_contract::GovernanceContract::vote_on_proposal(program_id, accounts, proposal_id, vote)
        }
        8 => {
            let amount = parse_amount(rest)?;
            let target_chain = String::from_utf8_lossy(&rest[8..]).to_string();
            cross_chain_bridge_contract::CrossChainBridge::lock_tokens_for_bridge(program_id, accounts, amount, &target_chain)
        }
        9 => {
            let amount = parse_amount(rest)?;
            let target_chain_address = String::from_utf8_lossy(&rest[8..]).to_string();
            let dummy_signature = vec![0u8; 64];
            cross_chain_bridge_contract::CrossChainBridge::release_tokens_on_target_chain(program_id, accounts, amount, &target_chain_address, &dummy_signature)
        }
        10 => {
            let client_requirements = String::from_utf8_lossy(rest).to_string();
            ai_contract::match_consultant(program_id, accounts, &client_requirements)
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

fn parse_amount(data: &[u8]) -> Result<u64, ProgramError> {
    if data.len() < 8 {
        return Err(ProgramError::InvalidInstructionData);
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[..8]);
    Ok(u64::from_le_bytes(bytes))
}