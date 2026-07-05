#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    Overflow,
    IssuerLimitExceeded,
    GlobalCapExceeded,
    OutstandingExceeded,
    RequestNotPending,
    Unauthorized,
    MissingPendingAdmin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IssuanceModel {
    pub global_cap: u64,
    pub issuer_limit: u64,
    pub current_supply: u64,
    pub issuer_outstanding: u64,
    pub total_minted: u64,
    pub total_burned: u64,
}

impl IssuanceModel {
    pub fn new(global_cap: u64, issuer_limit: u64) -> Self {
        Self {
            global_cap,
            issuer_limit,
            current_supply: 0,
            issuer_outstanding: 0,
            total_minted: 0,
            total_burned: 0,
        }
    }

    pub fn mint(&mut self, amount: u64) -> Result<(), ProtocolError> {
        let new_supply = self
            .current_supply
            .checked_add(amount)
            .ok_or(ProtocolError::Overflow)?;
        let new_outstanding = self
            .issuer_outstanding
            .checked_add(amount)
            .ok_or(ProtocolError::Overflow)?;
        let new_total_minted = self
            .total_minted
            .checked_add(amount)
            .ok_or(ProtocolError::Overflow)?;

        if new_supply > self.global_cap {
            return Err(ProtocolError::GlobalCapExceeded);
        }

        if new_outstanding > self.issuer_limit {
            return Err(ProtocolError::IssuerLimitExceeded);
        }

        self.current_supply = new_supply;
        self.issuer_outstanding = new_outstanding;
        self.total_minted = new_total_minted;

        Ok(())
    }

    pub fn burn(&mut self, amount: u64) -> Result<(), ProtocolError> {
        if amount > self.issuer_outstanding || amount > self.current_supply {
            return Err(ProtocolError::OutstandingExceeded);
        }

        self.current_supply = self
            .current_supply
            .checked_sub(amount)
            .ok_or(ProtocolError::OutstandingExceeded)?;
        self.issuer_outstanding = self
            .issuer_outstanding
            .checked_sub(amount)
            .ok_or(ProtocolError::OutstandingExceeded)?;
        self.total_burned = self
            .total_burned
            .checked_add(amount)
            .ok_or(ProtocolError::Overflow)?;

        Ok(())
    }

    pub fn assert_invariants(&self) {
        assert!(self.current_supply <= self.global_cap);
        assert!(self.issuer_outstanding <= self.issuer_limit);
        assert_eq!(self.total_minted - self.total_burned, self.current_supply);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedemptionStatus {
    Pending,
    Cancelled,
    Completed,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedemptionModel {
    pub amount: u64,
    pub status: RedemptionStatus,
    pub outstanding: u64,
    pub total_completed: u64,
    pub total_cancelled: u64,
    pub total_rejected: u64,
}

impl RedemptionModel {
    pub fn new(amount: u64) -> Self {
        Self {
            amount,
            status: RedemptionStatus::Pending,
            outstanding: amount,
            total_completed: 0,
            total_cancelled: 0,
            total_rejected: 0,
        }
    }

    pub fn cancel(&mut self) -> Result<(), ProtocolError> {
        self.settle(RedemptionStatus::Cancelled)
    }

    pub fn complete(&mut self) -> Result<(), ProtocolError> {
        self.settle(RedemptionStatus::Completed)
    }

    pub fn reject(&mut self) -> Result<(), ProtocolError> {
        self.settle(RedemptionStatus::Rejected)
    }

    fn settle(&mut self, next: RedemptionStatus) -> Result<(), ProtocolError> {
        if self.status != RedemptionStatus::Pending {
            return Err(ProtocolError::RequestNotPending);
        }

        self.outstanding = self
            .outstanding
            .checked_sub(self.amount)
            .ok_or(ProtocolError::OutstandingExceeded)?;
        self.status = next;

        match next {
            RedemptionStatus::Cancelled => self.total_cancelled = self.amount,
            RedemptionStatus::Completed => self.total_completed = self.amount,
            RedemptionStatus::Rejected => self.total_rejected = self.amount,
            RedemptionStatus::Pending => unreachable!(),
        }

        Ok(())
    }

    pub fn assert_invariants(&self) {
        let terminal_total = self.total_completed + self.total_cancelled + self.total_rejected;

        assert!(terminal_total <= self.amount);

        if self.status == RedemptionStatus::Pending {
            assert_eq!(self.outstanding, self.amount);
            assert_eq!(terminal_total, 0);
        } else {
            assert_eq!(self.outstanding, 0);
            assert_eq!(terminal_total, self.amount);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdminTransferModel {
    pub admin: u64,
    pub pending_admin: Option<u64>,
}

impl AdminTransferModel {
    pub fn new(admin: u64) -> Self {
        Self {
            admin,
            pending_admin: None,
        }
    }

    pub fn begin(&mut self, signer: u64, new_admin: u64) -> Result<(), ProtocolError> {
        if signer != self.admin {
            return Err(ProtocolError::Unauthorized);
        }

        self.pending_admin = Some(new_admin);
        Ok(())
    }

    pub fn accept(&mut self, signer: u64) -> Result<(), ProtocolError> {
        let pending_admin = self
            .pending_admin
            .ok_or(ProtocolError::MissingPendingAdmin)?;

        if signer != pending_admin {
            return Err(ProtocolError::Unauthorized);
        }

        self.admin = signer;
        self.pending_admin = None;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferHookAccounts {
    pub source_token_account: u64,
    pub destination_token_account: u64,
    pub source_compliance_user: u64,
    pub destination_compliance_user: u64,
}

impl TransferHookAccounts {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.source_token_account != self.source_compliance_user {
            return Err(ProtocolError::Unauthorized);
        }

        if self.destination_token_account != self.destination_compliance_user {
            return Err(ProtocolError::Unauthorized);
        }

        Ok(())
    }
}
