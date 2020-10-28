use super::*;

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
pub struct Exchange<T: Trait> {
    first_asset_pool: BalanceOf<T>,
    second_asset_pool: BalanceOf<T>,
    pub invariant: BalanceOf<T>,
    total_shares: BalanceOf<T>,
    shares: BTreeMap<T::AccountId, BalanceOf<T>>,
}

impl<T: Trait> Default for Exchange<T> {
    fn default() -> Self {
        Self {
            first_asset_pool: BalanceOf::<T>::default(),
            second_asset_pool: BalanceOf::<T>::default(),
            invariant: BalanceOf::<T>::default(),
            total_shares: BalanceOf::<T>::default(),
            shares: BTreeMap::new(),
        }
    }
}

impl<T: Trait> Exchange<T> {
    pub fn initialize_new(
        first_asset_amount: BalanceOf<T>,
        second_asset_amount: BalanceOf<T>,
        sender: T::AccountId,
    ) -> Self {
        let mut shares_map = BTreeMap::new();
        shares_map.insert(sender, T::InitialShares::get());
        Self {
            first_asset_pool: first_asset_amount,
            second_asset_pool: second_asset_amount,
            invariant: first_asset_amount * second_asset_amount,
            total_shares: T::InitialShares::get(),
            shares: shares_map,
        }
    }

    pub fn calculate_first_to_second_asset_swap(
        &self,
        first_asset_amount: BalanceOf<T>,
    ) -> (BalanceOf<T>, BalanceOf<T>, BalanceOf<T>) {
        let fee = first_asset_amount * T::ExchangeFeeRateNominator::get()
            / T::ExchangeFeeRateDenominator::get();
        let new_first_asset_pool = self.first_asset_pool + first_asset_amount;
        let temp_first_asset_pool = new_first_asset_pool - fee;
        let new_second_asset_pool = self.invariant / temp_first_asset_pool;
        let second_asset_amount = self.second_asset_pool - new_second_asset_pool;
        (
            new_first_asset_pool,
            new_second_asset_pool,
            second_asset_amount,
        )
    }

    pub fn calculate_second_to_first_asset_swap(
        &self,
        second_asset_amount: BalanceOf<T>,
    ) -> (BalanceOf<T>, BalanceOf<T>, BalanceOf<T>) {
        let fee = second_asset_amount * T::ExchangeFeeRateNominator::get()
            / T::ExchangeFeeRateDenominator::get();
        let new_second_asset_pool = self.second_asset_pool + second_asset_amount;
        let temp_second_asset_pool = new_second_asset_pool - fee;
        let new_first_asset_pool = self.invariant / temp_second_asset_pool;
        let first_asset_amount = self.first_asset_pool - new_first_asset_pool;
        (
            new_first_asset_pool,
            new_second_asset_pool,
            first_asset_amount,
        )
    }

    pub fn calculate_costs(&self, shares: BalanceOf<T>) -> (BalanceOf<T>, BalanceOf<T>) {
        let first_asset_cost = self.first_asset_pool * shares / self.total_shares;
        let second_asset_cost = self.second_asset_pool * shares / self.total_shares;

        (first_asset_cost, second_asset_cost)
    }

    pub fn invest(
        &mut self,
        first_asset_amount: BalanceOf<T>,
        second_asset_amount: BalanceOf<T>,
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
        self.first_asset_pool += first_asset_amount;
        self.second_asset_pool += second_asset_amount;
        self.invariant = self.first_asset_pool * self.second_asset_pool;
    }

    pub fn divest(
        &mut self,
        first_asset_amount: BalanceOf<T>,
        second_asset_amount: BalanceOf<T>,
        shares: BalanceOf<T>,
        sender: &T::AccountId,
    ) {
        if let Some(share) = self.shares.get_mut(sender) {
            *share -= shares;
        }

        self.total_shares -= shares;
        self.first_asset_pool -= first_asset_amount;
        self.second_asset_pool -= second_asset_amount;
        if self.total_shares == BalanceOf::<T>::zero() {
            self.invariant = BalanceOf::<T>::zero();
        } else {
            self.invariant = self.first_asset_pool * self.second_asset_pool;
        }
    }

    pub fn update_pools(
        &mut self,
        first_asset_pool: BalanceOf<T>,
        second_asset_pool: BalanceOf<T>,
    ) {
        self.first_asset_pool = first_asset_pool;
        self.second_asset_pool = second_asset_pool;
        self.invariant = self.first_asset_pool * self.second_asset_pool;
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

    pub fn ensure_second_asset_amount(
        &self,
        asset_out_amount: BalanceOf<T>,
        min_asset_out_amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        ensure!(
            asset_out_amount >= min_asset_out_amount,
            Error::<T>::AssetAmountBelowExpectation
        );
        ensure!(
            asset_out_amount <= self.second_asset_pool,
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

    pub fn ensure_first_asset_amount(
        &self,
        first_asset_out_amount: BalanceOf<T>,
        min_first_asset_out_amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        ensure!(
            first_asset_out_amount >= min_first_asset_out_amount,
            Error::<T>::AssetAmountBelowExpectation
        );
        ensure!(
            first_asset_out_amount <= self.first_asset_pool,
            Error::<T>::InsufficientPool
        );
        Ok(())
    }
}
