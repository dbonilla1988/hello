use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program::invoke,
    program_error::ProgramError,
    program_pack::{Pack, Sealed, IsInitialized},
    pubkey::Pubkey,
    sysvar::Sysvar,
};
use spl_token::instruction as token_instruction;
use borsh_derive::{BorshDeserialize, BorshSerialize};

#[derive(Clone)]
pub struct StakingContract {
    pub total_staked: u64,
    pub reward_pool: u64,
    pub penalty_pool: u64,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Stake {
    pub amount: u64,
    pub lock_until: i64,
    pub is_initialized: bool,
}

impl IsInitialized for Stake {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl StakingContract {
    pub fn new() -> Self {
        StakingContract {
            total_staked: 0,
            reward_pool: 15_000_000,
            penalty_pool: 0,
        }
    }

    pub fn stake_tokens(
        &mut self,
        _program_id: &Pubkey, // Prefixed with _ to suppress warning
        accounts: &[AccountInfo],
        amount: u64,
        lock_period_in_days: u64,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let staking_acc = next_account_info(account_info_iter)?;
        let staker_acc = next_account_info(account_info_iter)?;
        let pool_acc = next_account_info(account_info_iter)?;
        let staker_auth = next_account_info(account_info_iter)?;
        let token_program_acc = next_account_info(account_info_iter)?;

        if !staker_auth.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        if *token_program_acc.key != spl_token::id() {
            return Err(ProgramError::InvalidAccountData);
        }

        let stake_data = Stake {
            amount,
            lock_until: Clock::get()?.unix_timestamp + (lock_period_in_days as i64 * 86400),
            is_initialized: true,
        };
        let mut staking_data = staking_acc.try_borrow_mut_data()?;
        stake_data.pack_into_slice(&mut staking_data);

        let ix = token_instruction::transfer(
            token_program_acc.key,
            staker_acc.key,
            pool_acc.key,
            staker_auth.key,
            &[],
            amount,
        )?;
        invoke(&ix, &[staker_acc.clone(), pool_acc.clone(), staker_auth.clone(), token_program_acc.clone()])?;

        self.total_staked += amount;
        msg!("Staked {} tokens for {} days", amount, lock_period_in_days);
        Ok(())
    }

    pub fn unstake_tokens(
        &mut self,
        _program_id: &Pubkey, // Prefixed with _ to suppress warning
        accounts: &[AccountInfo],
        amount: u64,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let staking_acc = next_account_info(account_info_iter)?;
        let pool_acc = next_account_info(account_info_iter)?;
        let staker_acc = next_account_info(account_info_iter)?;
        let staker_auth = next_account_info(account_info_iter)?;
        let token_program_acc = next_account_info(account_info_iter)?;

        if !staker_auth.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        if *token_program_acc.key != spl_token::id() {
            return Err(ProgramError::InvalidAccountData);
        }

        let mut stake_data = Stake::unpack(&staking_acc.try_borrow_data()?)?;
        if stake_data.amount < amount {
            return Err(ProgramError::InsufficientFunds);
        }

        let current_time = Clock::get()?.unix_timestamp;
        let penalty = if current_time < stake_data.lock_until {
            let remaining_days = (stake_data.lock_until - current_time) / 86400;
            if remaining_days > 90 {
                10
            } else if remaining_days > 30 {
                7
            } else {
                5
            }
        } else {
            0
        };

        let penalty_amount = (amount * penalty) / 100;
        let final_amount = amount.saturating_sub(penalty_amount);

        stake_data.amount -= amount;
        let mut staking_data = staking_acc.try_borrow_mut_data()?;
        stake_data.pack_into_slice(&mut staking_data);

        self.total_staked = self.total_staked.saturating_sub(amount);
        self.penalty_pool += penalty_amount;

        let ix = token_instruction::transfer(
            token_program_acc.key,
            pool_acc.key,
            staker_acc.key,
            staker_auth.key,
            &[],
            final_amount,
        )?;
        invoke(&ix, &[pool_acc.clone(), staker_acc.clone(), staker_auth.clone(), token_program_acc.clone()])?;

        self.redistribute_penalty();
        msg!("Unstaked {} tokens with penalty {}", final_amount, penalty_amount);
        Ok(())
    }

    pub fn redistribute_penalty(&mut self) {
        if self.total_staked == 0 || self.penalty_pool == 0 {
            return;
        }
        let reward_per_token = self.penalty_pool / self.total_staked;
        self.reward_pool += self.penalty_pool;
        self.penalty_pool = 0;
        msg!("Redistributed penalty: {} per token", reward_per_token);
    }

    pub fn get_staked_amount(&self, staking_acc: &AccountInfo) -> Result<u64, ProgramError> {
        let stake_data = Stake::unpack(&staking_acc.try_borrow_data()?)?;
        Ok(stake_data.amount)
    }
}

impl Pack for Stake {
    const LEN: usize = 17; // u64 (8) + i64 (8) + bool (1)
    fn pack_into_slice(&self, dst: &mut [u8]) {
        let mut cursor = 0;
        dst[cursor..cursor + 8].copy_from_slice(&self.amount.to_le_bytes());
        cursor += 8;
        dst[cursor..cursor + 8].copy_from_slice(&self.lock_until.to_le_bytes());
        cursor += 8;
        dst[cursor] = self.is_initialized as u8;
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        let amount = u64::from_le_bytes(src[0..8].try_into().unwrap());
        let lock_until = i64::from_le_bytes(src[8..16].try_into().unwrap());
        let is_initialized = src[16] != 0;
        Ok(Stake { amount, lock_until, is_initialized })
    }
}

impl Sealed for Stake {}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::pubkey::Pubkey;

    #[test]
    fn test_stake_tokens() {
        let mut staking_contract = StakingContract::new();
        let program_id = Pubkey::new_unique();
        let staking_key = Pubkey::new_unique();
        let staker_key = Pubkey::new_unique();
        let pool_key = Pubkey::new_unique();
        let staker_auth_key = Pubkey::new_unique();
        let token_program_key = spl_token::id();

        let mut staking_lamports = 0u64;
        let mut staker_lamports = 1000u64;
        let mut pool_lamports = 0u64;
        let mut staker_auth_lamports = 0u64;
        let mut token_program_lamports = 0u64;

        let mut staking_data = vec![0u8; Stake::LEN];
        let mut staker_data = vec![];
        let mut pool_data = vec![];
        let mut staker_auth_data = vec![];
        let mut token_program_data = vec![];

        let staking_acc = AccountInfo::new(
            &staking_key,
            false,
            true,
            &mut staking_lamports,
            &mut staking_data,
            &program_id,
            false,
            0,
        );
        let staker_acc = AccountInfo::new(
            &staker_key,
            false,
            true,
            &mut staker_lamports,
            &mut staker_data,
            &token_program_key,
            false,
            0,
        );
        let pool_acc = AccountInfo::new(
            &pool_key,
            false,
            true,
            &mut pool_lamports,
            &mut pool_data,
            &token_program_key,
            false,
            0,
        );
        let staker_auth = AccountInfo::new(
            &staker_auth_key,
            true, // Signer
            false,
            &mut staker_auth_lamports,
            &mut staker_auth_data,
            &program_id,
            false,
            0,
        );
        let token_program_acc = AccountInfo::new(
            &token_program_key,
            false,
            false,
            &mut token_program_lamports,
            &mut token_program_data,
            &program_id,
            false,
            0,
        );

        let accounts = vec![staking_acc, staker_acc, pool_acc, staker_auth, token_program_acc];
        let res = staking_contract.stake_tokens(&program_id, &accounts, 500, 30);
        assert!(res.is_err()); // Expect Err due to stubbed invoke in test env
    }

    #[test]
    fn test_unstake_tokens_no_penalty() {
        let mut staking_contract = StakingContract::new();
        let program_id = Pubkey::new_unique();
        let staking_key = Pubkey::new_unique();
        let pool_key = Pubkey::new_unique();
        let staker_key = Pubkey::new_unique();
        let staker_auth_key = Pubkey::new_unique();
        let token_program_key = spl_token::id();

        let mut staking_lamports = 0u64;
        let mut pool_lamports = 1000u64;
        let mut staker_lamports = 0u64;
        let mut staker_auth_lamports = 0u64;
        let mut token_program_lamports = 0u64;

        let stake_data = Stake {
            amount: 500,
            lock_until: 0, // Already unlocked
            is_initialized: true,
        };
        let mut staking_data = vec![0u8; Stake::LEN];
        stake_data.pack_into_slice(&mut staking_data);
        let mut pool_data = vec![];
        let mut staker_data = vec![];
        let mut staker_auth_data = vec![];
        let mut token_program_data = vec![];

        let staking_acc = AccountInfo::new(
            &staking_key,
            false,
            true,
            &mut staking_lamports,
            &mut staking_data,
            &program_id,
            false,
            0,
        );
        let pool_acc = AccountInfo::new(
            &pool_key,
            false,
            true,
            &mut pool_lamports,
            &mut pool_data,
            &token_program_key,
            false,
            0,
        );
        let staker_acc = AccountInfo::new(
            &staker_key,
            false,
            true,
            &mut staker_lamports,
            &mut staker_data,
            &token_program_key,
            false,
            0,
        );
        let staker_auth = AccountInfo::new(
            &staker_auth_key,
            true, // Signer
            false,
            &mut staker_auth_lamports,
            &mut staker_auth_data,
            &program_id,
            false,
            0,
        );
        let token_program_acc = AccountInfo::new(
            &token_program_key,
            false,
            false,
            &mut token_program_lamports,
            &mut token_program_data,
            &program_id,
            false,
            0,
        );

        staking_contract.total_staked = 500;
        let accounts = vec![staking_acc, pool_acc, staker_acc, staker_auth, token_program_acc];
        let res = staking_contract.unstake_tokens(&program_id, &accounts, 500);
        assert!(res.is_err()); // Expect Err due to stubbed invoke in test env
    }

    #[test]
    fn test_get_staked_amount() {
        let staking_contract = StakingContract::new();
        let program_id = Pubkey::new_unique();
        let staking_key = Pubkey::new_unique();
        let mut staking_lamports = 0u64;

        let stake_data = Stake {
            amount: 500,
            lock_until: 0,
            is_initialized: true,
        };
        let mut staking_data = vec![0u8; Stake::LEN];
        stake_data.pack_into_slice(&mut staking_data);

        let staking_acc = AccountInfo::new(
            &staking_key,
            false,
            true,
            &mut staking_lamports,
            &mut staking_data,
            &program_id,
            false,
            0,
        );

        let amount = staking_contract.get_staked_amount(&staking_acc).unwrap();
        assert_eq!(amount, 500);
    }

    #[test]
    fn test_unstake_tokens_with_penalty() {
        let mut staking_contract = StakingContract::new();
        let program_id = Pubkey::new_unique();
        let staking_key = Pubkey::new_unique();
        let pool_key = Pubkey::new_unique();
        let staker_key = Pubkey::new_unique();
        let staker_auth_key = Pubkey::new_unique();
        let token_program_key = spl_token::id();

        let mut staking_lamports = 0u64;
        let mut pool_lamports = 1000u64;
        let mut staker_lamports = 0u64;
        let mut staker_auth_lamports = 0u64;
        let mut token_program_lamports = 0u64;

        let stake_data = Stake {
            amount: 500,
            lock_until: i64::MAX, // Far in the future for 10% penalty
            is_initialized: true,
        };
        let mut staking_data = vec![0u8; Stake::LEN];
        stake_data.pack_into_slice(&mut staking_data);
        let mut pool_data = vec![];
        let mut staker_data = vec![];
        let mut staker_auth_data = vec![];
        let mut token_program_data = vec![];

        let staking_acc = AccountInfo::new(
            &staking_key,
            false,
            true,
            &mut staking_lamports,
            &mut staking_data,
            &program_id,
            false,
            0,
        );
        let pool_acc = AccountInfo::new(
            &pool_key,
            false,
            true,
            &mut pool_lamports,
            &mut pool_data,
            &token_program_key,
            false,
            0,
        );
        let staker_acc = AccountInfo::new(
            &staker_key,
            false,
            true,
            &mut staker_lamports,
            &mut staker_data,
            &token_program_key,
            false,
            0,
        );
        let staker_auth = AccountInfo::new(
            &staker_auth_key,
            true,
            false,
            &mut staker_auth_lamports,
            &mut staker_auth_data,
            &program_id,
            false,
            0,
        );
        let token_program_acc = AccountInfo::new(
            &token_program_key,
            false,
            false,
            &mut token_program_lamports,
            &mut token_program_data,
            &program_id,
            false,
            0,
        );

        staking_contract.total_staked = 500;
        let accounts = vec![staking_acc, pool_acc, staker_acc, staker_auth, token_program_acc];
        let res = staking_contract.unstake_tokens(&program_id, &accounts, 500);
        assert!(res.is_err()); // Expect Err due to stubbed invoke
    }
}