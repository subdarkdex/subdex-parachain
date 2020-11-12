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
    // Avoid casting to float
    fn sqrt(y: BalanceOf<T>) -> BalanceOf<T> {
        if y > 3.into() {
            let mut z = y;
            let mut x = y / 2.into() + 1.into();
            while x < z {
                z = x;
                x = (y / x + x) / 2.into();
            }
            return z;
        } else if y != BalanceOf::<T>::zero() {
            BalanceOf::<T>::one()
        } else {
            BalanceOf::<T>::zero()
        }
    }

    fn get_min_fee() -> BalanceOf<T> {
        match core::mem::size_of::<BalanceOf<T>>() {
            size if size <= 64 => 1.into(),
            // cosider 112 instead
            size if size > 64 && size < 128 => 10.into(),
            _ => (10 * 10 * 10).into(),
        }
    }

    pub fn initialize_new(
        first_asset_amount: BalanceOf<T>,
        second_asset_amount: BalanceOf<T>,
        sender: T::AccountId,
    ) -> Result<Self, Error<T>> {
        let mut shares_map = BTreeMap::new();
        let min_fee = Self::get_min_fee();

        let initial_shares = Self::sqrt(first_asset_amount * second_asset_amount)
            .checked_sub(&min_fee)
            .ok_or(Error::<T>::UnderflowOccured)?;

        shares_map.insert(sender, initial_shares);
        let exchange = Self {
            first_asset_pool: first_asset_amount,
            second_asset_pool: second_asset_amount,
            invariant: first_asset_amount
                .checked_mul(&second_asset_amount)
                .ok_or(Error::<T>::UnderflowOrOverflowOccured)?,
            total_shares: initial_shares,
            shares: shares_map,
        };
        Ok(exchange)
    }

    pub fn calculate_first_to_second_asset_swap(
        &self,
        first_asset_amount: BalanceOf<T>,
    ) -> Result<(BalanceOf<T>, BalanceOf<T>, BalanceOf<T>), Error<T>> {
        let fee = T::ExchangeFeeRateNominator::get()
            .checked_div(&T::ExchangeFeeRateDenominator::get())
            .map(|fee_rate| fee_rate.checked_mul(&first_asset_amount))
            .flatten()
            .ok_or(Error::<T>::UnderflowOrOverflowOccured)?;
        let new_first_asset_pool = self
            .first_asset_pool
            .checked_add(&first_asset_amount)
            .ok_or(Error::<T>::OverflowOccured)?;
        let temp_first_asset_pool = new_first_asset_pool
            .checked_sub(&fee)
            .ok_or(Error::<T>::UnderflowOccured)?;
        let new_second_asset_pool = self
            .invariant
            .checked_div(&temp_first_asset_pool)
            .ok_or(Error::<T>::UnderflowOrOverflowOccured)?;
        let second_asset_amount = self
            .second_asset_pool
            .checked_sub(&new_second_asset_pool)
            .ok_or(Error::<T>::UnderflowOccured)?;
        Ok((
            new_first_asset_pool,
            new_second_asset_pool,
            second_asset_amount,
        ))
    }

    pub fn calculate_second_to_first_asset_swap(
        &self,
        second_asset_amount: BalanceOf<T>,
    ) -> Result<(BalanceOf<T>, BalanceOf<T>, BalanceOf<T>), Error<T>> {
        let fee = T::ExchangeFeeRateNominator::get()
            .checked_div(&T::ExchangeFeeRateDenominator::get())
            .map(|fee_rate| fee_rate.checked_mul(&second_asset_amount))
            .flatten()
            .ok_or(Error::<T>::UnderflowOrOverflowOccured)?;
        let new_second_asset_pool = self
            .second_asset_pool
            .checked_add(&second_asset_amount)
            .ok_or(Error::<T>::OverflowOccured)?;
        let temp_second_asset_pool = new_second_asset_pool
            .checked_sub(&fee)
            .ok_or(Error::<T>::UnderflowOccured)?;
        let new_first_asset_pool = self
            .invariant
            .checked_div(&temp_second_asset_pool)
            .ok_or(Error::<T>::UnderflowOrOverflowOccured)?;
        let first_asset_amount = self
            .first_asset_pool
            .checked_sub(&new_first_asset_pool)
            .ok_or(Error::<T>::UnderflowOccured)?;
        Ok((
            new_first_asset_pool,
            new_second_asset_pool,
            first_asset_amount,
        ))
    }

    pub fn calculate_costs(
        &self,
        shares: BalanceOf<T>,
    ) -> Result<(BalanceOf<T>, BalanceOf<T>), Error<T>> {
        let first_asset_cost = shares
            .checked_div(&self.total_shares)
            .map(|ratio| ratio * self.first_asset_pool)
            .ok_or(Error::<T>::UnderflowOrOverflowOccured)?;
        let second_asset_cost = shares
            .checked_div(&self.total_shares)
            .map(|ratio| ratio * self.second_asset_pool)
            .ok_or(Error::<T>::UnderflowOrOverflowOccured)?;

        Ok((first_asset_cost, second_asset_cost))
    }

    pub fn invest(
        &mut self,
        first_asset_amount: BalanceOf<T>,
        second_asset_amount: BalanceOf<T>,
        shares: BalanceOf<T>,
        sender: &T::AccountId,
    ) -> Result<(), Error<T>> {
        let updated_shares = if let Some(prev_shares) = self.shares.get(sender) {
            prev_shares
                .checked_add(&shares)
                .ok_or(Error::<T>::OverflowOccured)?
        } else {
            shares
        };

        self.shares.insert(sender.clone(), updated_shares);
        self.total_shares = self
            .total_shares
            .checked_add(&shares)
            .ok_or(Error::<T>::OverflowOccured)?;
        self.first_asset_pool = self
            .first_asset_pool
            .checked_add(&first_asset_amount)
            .ok_or(Error::<T>::OverflowOccured)?;
        self.second_asset_pool = self
            .second_asset_pool
            .checked_add(&second_asset_amount)
            .ok_or(Error::<T>::OverflowOccured)?;
        self.invariant = self
            .first_asset_pool
            .checked_mul(&self.second_asset_pool)
            .ok_or(Error::<T>::UnderflowOrOverflowOccured)?;
        Ok(())
    }

    pub fn divest(
        &mut self,
        first_asset_amount: BalanceOf<T>,
        second_asset_amount: BalanceOf<T>,
        shares: BalanceOf<T>,
        sender: &T::AccountId,
    ) -> Result<(), Error<T>> {
        if let Some(share) = self.shares.get_mut(sender) {
            share
                .checked_sub(&shares)
                .ok_or(Error::<T>::UnderflowOccured)?;
        }

        self.total_shares = self
            .total_shares
            .checked_sub(&shares)
            .ok_or(Error::<T>::UnderflowOccured)?;
        self.first_asset_pool = self
            .first_asset_pool
            .checked_sub(&first_asset_amount)
            .ok_or(Error::<T>::UnderflowOccured)?;
        self.second_asset_pool = self
            .second_asset_pool
            .checked_sub(&second_asset_amount)
            .ok_or(Error::<T>::UnderflowOccured)?;
        if self.total_shares == BalanceOf::<T>::zero() {
            self.invariant = BalanceOf::<T>::zero();
        } else {
            self.invariant = self
                .first_asset_pool
                .checked_mul(&self.second_asset_pool)
                .ok_or(Error::<T>::UnderflowOrOverflowOccured)?;
        }
        Ok(())
    }

    pub fn update_pools(
        &mut self,
        first_asset_pool: BalanceOf<T>,
        second_asset_pool: BalanceOf<T>,
    ) -> Result<(), Error<T>> {
        self.first_asset_pool = first_asset_pool;
        self.second_asset_pool = second_asset_pool;
        self.invariant = self
            .first_asset_pool
            .checked_mul(&self.second_asset_pool)
            .ok_or(Error::<T>::UnderflowOrOverflowOccured)?;
        Ok(())
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
            Error::<T>::SecondAssetAmountBelowExpectation
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
            Error::<T>::SecondAssetAmountBelowExpectation
        );
        ensure!(
            first_asset_out_amount <= self.first_asset_pool,
            Error::<T>::InsufficientPool
        );
        Ok(())
    }
}
