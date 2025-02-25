use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::{Pack, Sealed, IsInitialized},
    pubkey::Pubkey,
    sysvar::Sysvar,
};
use borsh_derive::{BorshDeserialize, BorshSerialize};
use crate::{staking_contract::StakingContract, ADMIN_PUBKEY, GOVERNANCE_PUBKEY};


#[derive(BorshSerialize, BorshDeserialize)]
pub struct Proposal {
    pub description: String,
    pub proposer: Pubkey,
    pub active: bool,
    pub timestamp: i64,
    pub is_initialized: bool,
}

impl Sealed for Proposal {}

impl IsInitialized for Proposal {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl Pack for Proposal {
    const LEN: usize = 300; // Adjust based on max description length + fields
    fn pack_into_slice(&self, dst: &mut [u8]) {
        let mut cursor = 0;
        let desc_bytes = self.description.as_bytes();
        let desc_len = desc_bytes.len() as u32;
        dst[cursor..cursor + 4].copy_from_slice(&desc_len.to_le_bytes());
        cursor += 4;
        dst[cursor..cursor + desc_bytes.len()].copy_from_slice(desc_bytes);
        cursor += desc_bytes.len();
        dst[cursor..cursor + 32].copy_from_slice(self.proposer.as_ref());
        cursor += 32;
        dst[cursor] = self.active as u8;
        cursor += 1;
        dst[cursor..cursor + 8].copy_from_slice(&self.timestamp.to_le_bytes());
        cursor += 8;
        dst[cursor] = self.is_initialized as u8;
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() < 45 {
            return Err(ProgramError::InvalidAccountData);
        }
        let mut cursor = 0;
        let desc_len = u32::from_le_bytes(src[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;
        if cursor + desc_len > src.len() {
            return Err(ProgramError::InvalidAccountData);
        }
        let description = String::from_utf8(src[cursor..cursor + desc_len].to_vec())
            .map_err(|_| ProgramError::InvalidAccountData)?;
        cursor += desc_len;
        let proposer = Pubkey::new_from_array(src[cursor..cursor + 32].try_into().unwrap());
        cursor += 32;
        let active = src[cursor] != 0;
        cursor += 1;
        let timestamp = i64::from_le_bytes(src[cursor..cursor + 8].try_into().unwrap());
        cursor += 8;
        let is_initialized = src[cursor] != 0;
        Ok(Proposal {
            description,
            proposer,
            active,
            timestamp,
            is_initialized,
        })
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Vote {
    pub proposal: Pubkey,
    pub voter: Pubkey,
    pub vote: bool,
    pub weight: u64,
    pub is_initialized: bool,
}

impl Sealed for Vote {}

impl IsInitialized for Vote {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl Pack for Vote {
    const LEN: usize = 73; // Pubkey (32) + Pubkey (32) + bool (1) + u64 (8) + bool (1)
    fn pack_into_slice(&self, dst: &mut [u8]) {
        let mut cursor = 0;
        dst[cursor..cursor + 32].copy_from_slice(self.proposal.as_ref());
        cursor += 32;
        dst[cursor..cursor + 32].copy_from_slice(self.voter.as_ref());
        cursor += 32;
        dst[cursor] = self.vote as u8;
        cursor += 1;
        dst[cursor..cursor + 8].copy_from_slice(&self.weight.to_le_bytes());
        cursor += 8;
        dst[cursor] = self.is_initialized as u8;
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        let mut cursor = 0;
        let proposal = Pubkey::new_from_array(src[cursor..cursor + 32].try_into().unwrap());
        cursor += 32;
        let voter = Pubkey::new_from_array(src[cursor..cursor + 32].try_into().unwrap());
        cursor += 32;
        let vote = src[cursor] != 0;
        cursor += 1;
        let weight = u64::from_le_bytes(src[cursor..cursor + 8].try_into().unwrap());
        cursor += 8;
        let is_initialized = src[cursor] != 0;
        Ok(Vote {
            proposal,
            voter,
            vote,
            weight,
            is_initialized,
        })
    }
}

pub struct GovernanceContract;

impl GovernanceContract {
    pub fn create_proposal(_program_id: &Pubkey, accounts: &[AccountInfo], description: &str) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let proposal_acc = next_account_info(account_info_iter)?;
        let proposer_acc = next_account_info(account_info_iter)?;

        if !proposer_acc.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let proposal = Proposal {
            description: description.to_string(),
            proposer: *proposer_acc.key,
            active: true,
            timestamp: Clock::get()?.unix_timestamp,
            is_initialized: true,
        };
        let mut proposal_data = proposal_acc.try_borrow_mut_data()?;
        proposal.pack_into_slice(&mut proposal_data);
        msg!("Created proposal: {}", description);
        Ok(())
    }

    pub fn execute_proposal(_program_id: &Pubkey, accounts: &[AccountInfo], _proposal_id: u64) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let proposal_acc = next_account_info(account_info_iter)?;
        let authority_acc = next_account_info(account_info_iter)?;

        if authority_acc.key != &ADMIN_PUBKEY && authority_acc.key != &GOVERNANCE_PUBKEY {
            return Err(ProgramError::IllegalOwner);
        }
        if !authority_acc.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mut proposal = Proposal::unpack(&proposal_acc.try_borrow_data()?)?;
        if !proposal.active {
            return Err(ProgramError::InvalidArgument);
        }
        proposal.active = false;
        let mut proposal_data = proposal_acc.try_borrow_mut_data()?;
        proposal.pack_into_slice(&mut proposal_data);
        msg!("Executing proposal with ID: {}", _proposal_id);
        Ok(())
    }

    pub fn vote_on_proposal(_program_id: &Pubkey, accounts: &[AccountInfo], _proposal_id: u64, vote_in_favor: bool) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let vote_acc = next_account_info(account_info_iter)?;
        let voter_acc = next_account_info(account_info_iter)?;
        let proposal_acc = next_account_info(account_info_iter)?;
        let staking_acc = next_account_info(account_info_iter)?;

        if !voter_acc.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let proposal = Proposal::unpack(&proposal_acc.try_borrow_data()?)?;
        if !proposal.active {
            return Err(ProgramError::InvalidArgument);
        }

        let staking_contract = StakingContract::new();
        let staked_amount = staking_contract.get_staked_amount(staking_acc).unwrap_or(0);

        let vote_data = Vote {
            proposal: *proposal_acc.key,
            voter: *voter_acc.key,
            vote: vote_in_favor,
            weight: staked_amount,
            is_initialized: true,
        };
        let mut vote_data_mut = vote_acc.try_borrow_mut_data()?;
        vote_data.pack_into_slice(&mut vote_data_mut);
        msg!("Voted {} on proposal {} with weight {}", vote_in_favor, _proposal_id, staked_amount);
        Ok(())
    }
}