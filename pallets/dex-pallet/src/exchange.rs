use super::*;

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
pub struct Exchange<T: Trait> {
    first_currency_pool: BalanceOf<T>,
    second_currency_pool: BalanceOf<T>,
    pub invariant: BalanceOf<T>,
    total_shares: BalanceOf<T>,
    shares: BTreeMap<T::AccountId, BalanceOf<T>>,
}

impl<T: Trait> Default for Exchange<T> {
    fn default() -> Self {
        Self {
            first_currency_pool: BalanceOf::<T>::default(),
            second_currency_pool: BalanceOf::<T>::default(),
            invariant: BalanceOf::<T>::default(),
            total_shares: BalanceOf::<T>::default(),
            shares: BTreeMap::new(),
        }
    }
}

impl<T: Trait> Exchange<T> {
    pub fn initialize_new(
        first_currency_amount: BalanceOf<T>,
        second_currency_amount: BalanceOf<T>,
        sender: T::AccountId,
    ) -> Self {
        let mut shares_map = BTreeMap::new();
        shares_map.insert(sender, T::InitialShares::get());
        Self {
            first_currency_pool: first_currency_amount,
            second_currency_pool: second_currency_amount,
            invariant: first_currency_amount * second_currency_amount,
            total_shares: T::InitialShares::get(),
            shares: shares_map,
        }
    }

    pub fn calculate_ksm_to_token_swap(
        &self,
        first_currency_amount: BalanceOf<T>,
    ) -> (BalanceOf<T>, BalanceOf<T>, BalanceOf<T>) {
        let fee = first_currency_amount * T::ExchangeFeeRateNominator::get()
            / T::ExchangeFeeRateDenominator::get();
        let new_ksm_pool = self.first_currency_pool + first_currency_amount;
        let temp_ksm_pool = new_ksm_pool - fee;
        let new_token_pool = self.invariant / temp_ksm_pool;
        let second_currency_amount = self.second_currency_pool - new_token_pool;
        (new_ksm_pool, new_token_pool, second_currency_amount)
    }

    pub fn calculate_token_to_ksm_swap(
        &self,
        second_currency_amount: BalanceOf<T>,
    ) -> (BalanceOf<T>, BalanceOf<T>, BalanceOf<T>) {
        let fee = second_currency_amount * T::ExchangeFeeRateNominator::get()
            / T::ExchangeFeeRateDenominator::get();
        let new_token_pool = self.second_currency_pool + second_currency_amount;
        let temp_token_pool = new_token_pool - fee;
        let new_ksm_pool = self.invariant / temp_token_pool;
        let first_currency_amount = self.first_currency_pool - new_ksm_pool;
        (new_ksm_pool, new_token_pool, first_currency_amount)
    }

    pub fn calculate_costs(&self, shares: BalanceOf<T>) -> (BalanceOf<T>, BalanceOf<T>) {
        let first_currency_cost = self.first_currency_pool * shares / self.total_shares;
        let second_currency_cost = self.second_currency_pool * shares / self.total_shares;

        (first_currency_cost, second_currency_cost)
    }

    pub fn invest(
        &mut self,
        first_currency_amount: BalanceOf<T>,
        second_currency_amount: BalanceOf<T>,
        shares: BalanceOf<T>,
        sender: &T::AccountId,
    ) {
        let updated_shares = if let Some(prev_shares) = self.shares.get(sender) {
            *prev_shares + shares
        } else {
            shares
        };
        self.shares.insert(sender.clone(), updated_shares);
        self.total_shares += shares;
        self.first_currency_pool += first_currency_amount;
        self.second_currency_pool += second_currency_amount;
        self.invariant = self.first_currency_pool * self.second_currency_pool;
    }

    pub fn divest(
        &mut self,
        first_currency_amount: BalanceOf<T>,
        second_currency_amount: BalanceOf<T>,
        shares: BalanceOf<T>,
        sender: &T::AccountId,
    ) {
        if let Some(share) = self.shares.get_mut(sender) {
            *share -= shares;
        }

        self.total_shares -= shares;
        self.first_currency_pool -= first_currency_amount;
        self.second_currency_pool -= second_currency_amount;
        if self.total_shares == BalanceOf::<T>::zero() {
            self.invariant = BalanceOf::<T>::zero();
        } else {
            self.invariant = self.first_currency_pool * self.second_currency_pool;
        }
    }

    pub fn update_pools(
        &mut self,
        first_currency_pool: BalanceOf<T>,
        second_currency_pool: BalanceOf<T>,
    ) {
        self.first_currency_pool = first_currency_pool;
        self.second_currency_pool = second_currency_pool;
        self.invariant = self.first_currency_pool * self.second_currency_pool;
    }

    pub fn ensure_launch(&self) -> dispatch::DispatchResult {
        ensure!(
            self.invariant == BalanceOf::<T>::zero(),
            Error::<T>::InvariantNotNull
        );
        ensure!(
            self.total_shares == BalanceOf::<T>::zero(),
            Error::<T>::TotalSharesNotNull
        );
        Ok(())
    }

    pub fn ensure_token_amount(
        &self,
        token_out_amount: BalanceOf<T>,
        min_token_out_amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        ensure!(
            token_out_amount >= min_token_out_amount,
            Error::<T>::TokenAmountBelowExpectation
        );
        ensure!(
            token_out_amount <= self.second_currency_pool,
            Error::<T>::InsufficientPool
        );
        Ok(())
    }

    pub fn ensure_burned_shares(
        &self,
        sender: &T::AccountId,
        shares_burned: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        ensure!(
            shares_burned > BalanceOf::<T>::zero(),
            Error::<T>::InvalidShares
        );
        if let Some(shares) = self.shares.get(sender) {
            ensure!(*shares >= shares_burned, Error::<T>::InsufficientShares);
            Ok(())
        } else {
            Err(Error::<T>::DoesNotOwnShare.into())
        }
    }

    pub fn ensure_ksm_amount(
        &self,
        ksm_out_amount: BalanceOf<T>,
        min_ksm_out_amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        ensure!(
            ksm_out_amount >= min_ksm_out_amount,
            Error::<T>::KsmAmountBelowExpectation
        );
        ensure!(
            ksm_out_amount <= self.first_currency_pool,
            Error::<T>::InsufficientPool
        );
        Ok(())
    }
}
