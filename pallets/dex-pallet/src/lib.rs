#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Codec, Decode, Encode};
use frame_support::traits::{Currency, Imbalance};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch, ensure,
    traits::{Get, WithdrawReason},
    Parameter,
};
use frame_system::{self as system, ensure_signed};
use sp_arithmetic::traits::{BaseArithmetic, Zero};
use sp_runtime::traits::{
    CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, MaybeSerializeDeserialize, Member,
};

use sp_std::{collections::btree_map::BTreeMap, fmt::Debug, prelude::*};

mod exchange;
use exchange::Exchange;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
pub use serde::{Deserialize, Serialize};

pub type BalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;

pub trait Trait: system::Trait + balances::Trait {
    type Event: From<Event<Self>>
        + Into<<Self as system::Trait>::Event>
        + Into<<Self as balances::Trait>::Event>;

    type Currency: Currency<Self::AccountId>;

    type InitialShares: Get<BalanceOf<Self>>;

    // Id representation for assets, located on other parachains.
    // Some ids can be reserved to specify internal assets.
    type AssetId: Parameter
        + Member
        + BaseArithmetic
        + Codec
        + Default
        + Copy
        + Clone
        + MaybeSerializeDeserialize
        + Eq
        + PartialEq
        + Ord;

    type KSMAssetId: Get<Self::AssetId>;

    type ExchangeFeeRateNominator: Get<BalanceOf<Self>>;

    type ExchangeFeeRateDenominator: Get<BalanceOf<Self>>;
}

decl_storage! {
    trait Store for Module<T: Trait> as TemplateModule {
        pub Exchanges get(fn exchanges): double_map hasher(blake2_128_concat) T::AssetId, hasher(blake2_128_concat) T::AssetId => Exchange<T>;

        // Balances of assets, located on other parachains.
        pub AssetBalances get(fn asset_balances): double_map hasher(blake2_128_concat) T::AccountId, hasher(blake2_128_concat) T::AssetId => BalanceOf<T>;

        // Treasury account
        pub DEXAccountId get(fn dex_account_id) config(): T::AccountId;

        // Next asset id
        pub NextAssetId get(fn next_asset_id) config(): T::AssetId;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
        AssetId = <T as Trait>::AssetId,
        Shares = BalanceOf<T>,
        Balance = BalanceOf<T>,
    {
        Exchanged(AssetId, Balance, AssetId, Balance, AccountId),
        Invested(AccountId, AssetId, AssetId, Shares),
        Divested(AccountId, AssetId, AssetId, Shares),
        Withdrawn(AssetId, Balance, AccountId),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        ExchangeNotExists,
        ExchangeAlreadyExists,
        InvalidExchange,
        InvariantNotNull,
        TotalSharesNotNull,
        LowKsmAmount,
        LowassetAmount,
        KsmAmountBelowExpectation,
        AssetAmountBelowExpectation,
        InsufficientPool,
        InvalidShares,
        InsufficientShares,
        DoesNotOwnShare,
        InsufficientKsmBalance,
        InsufficientOtherAssetBalance,

        OverflowOccured
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {

        type Error = Error<T>;

        fn deposit_event() = default;

        #[weight = 10_000]
        pub fn initialize_exchange(origin, first_asset_id: T::AssetId, first_asset_amount: BalanceOf<T>, second_asset_id: T::AssetId, second_asset_amount: BalanceOf<T>) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            let (first_asset_id, first_asset_amount, second_asset_id, second_asset_amount) =
                Self::adjust_assets_amount_order(first_asset_id, first_asset_amount, second_asset_id, second_asset_amount);

            ensure!(
                first_asset_amount > BalanceOf::<T>::zero(),
                Error::<T>::LowKsmAmount
            );
            ensure!(
                second_asset_amount > BalanceOf::<T>::zero(),
                Error::<T>::LowassetAmount
            );

            Self::ensure_exchange_not_exists(first_asset_id, second_asset_id)?;
            Self::exchanges(first_asset_id, second_asset_id).ensure_launch()?;
            Self::ensure_sufficient_balances(&sender, first_asset_id, first_asset_amount, second_asset_id, second_asset_amount)?;

            //
            // == MUTATION SAFE ==
            //

            Self::slash_assets(&sender, first_asset_id, first_asset_amount, second_asset_id, second_asset_amount);

            // TODO adjust shares allocation
            let exchange = Exchange::<T>::initialize_new(first_asset_amount, second_asset_amount, sender.clone());

            Exchanges::<T>::insert(first_asset_id, second_asset_id, exchange);

            Self::deposit_event(RawEvent::Invested(sender, first_asset_id, second_asset_id, T::InitialShares::get()));
            Ok(())
        }

        #[weight = 10_000]
        pub fn swap_to_exact(
            origin,
            asset_in: T::AssetId,
            asset_in_amount: BalanceOf<T>,
            asset_out: T::AssetId,
            min_asset_out_amount: BalanceOf<T>,
            receiver: T::AccountId
        ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            Self::ensure_valid_exchange(asset_in, asset_out)?;

            let (adjusted_first_asset_id, adjusted_second_asset_id, adjsuted) = Self::adjust_assets_order(asset_in, asset_out);

            let exchange = Self::ensure_exchange_exists(adjusted_first_asset_id, adjusted_second_asset_id)?;

            Self::ensure_sufficient_balance(&sender, asset_in, asset_in_amount)?;

            let (new_first_asset_pool, new_second_asset_pool, asset_out_amount) = if !adjsuted {
                let (new_first_asset_pool, new_second_asset_pool, second_asset_out_amount) =
                    exchange.calculate_first_to_second_asset_swap(asset_in_amount);

                    exchange.ensure_second_asset_amount(second_asset_out_amount, min_asset_out_amount)?;

                    Self::ensure_can_hold_balance(&sender, asset_out, second_asset_out_amount)?;

                    (new_first_asset_pool, new_second_asset_pool, second_asset_out_amount)
            } else {
                let (new_first_asset_pool, new_second_asset_pool, first_asset_out_amount) =
                    exchange.calculate_second_to_first_asset_swap(asset_in_amount);

                    exchange.ensure_first_asset_amount(first_asset_out_amount, min_asset_out_amount)?;

                    Self::ensure_can_hold_balance(&sender, asset_out, first_asset_out_amount)?;

                    (new_first_asset_pool, new_second_asset_pool, first_asset_out_amount)
            };

            //
            // == MUTATION SAFE ==
            //

            Self::slash_asset(&sender, asset_in, asset_in_amount);

            Self::mint_asset(&sender, asset_out, asset_out_amount);

            <Exchanges<T>>::mutate(adjusted_first_asset_id, adjusted_second_asset_id, |exchange| {
                exchange.update_pools(new_first_asset_pool, new_second_asset_pool)
            });

            Self::deposit_event(RawEvent::Exchanged(
                asset_in,
                asset_in_amount,
                asset_out,
                asset_out_amount,
                sender,
            ));
            Ok(())
        }

        // #[weight = 10_000]
        // pub fn swap_exact_to(
        //     origin,
        //     asset_in: T::AssetId,
        //     asset_in_amount: BalanceOf<T>,
        //     asset_out: T::AssetId,
        //     min_asset_out_amount: BalanceOf<T>,
        //     receiver: T::AccountId
        // ) -> dispatch::DispatchResult {
        //     let sender = ensure_signed(origin)?;

        //     Self::ensure_valid_exchange(asset_in, asset_out)?;

        //     let from_exchange = Self::ensure_exchange_exists(asset_in)?;

        //     let to_exchange = Self::ensure_exchange_exists(asset_out)?;

        //     let (new_first_asset_pool_from, new_second_asset_pool_from, first_asset_amount) =
        //         from_exchange.calculate_asset_to_ksm_swap(asset_in_amount);
        //     from_exchange.ensure_ksm_amount(first_asset_amount, BalanceOf::<T>::zero())?;

        //     let (new_first_asset_pool_to, new_second_asset_pool_to, asset_out_amount) =
        //         to_exchange.calculate_ksm_to_asset_swap(first_asset_amount);
        //     to_exchange.ensure_asset_amount(asset_out_amount, min_asset_out_amount)?;
        //     Self::ensure_sufficient_balance(&sender, &asset_in, asset_in_amount)?;
        //     Self::ensure_sufficient_balance(&Self::dex_account_id(), &asset_out, asset_out_amount)?;

        //     //
        //     // == MUTATION SAFE ==
        //     //

        //     // transfer `second_asset_amount` to the DEX account
        //     <balances::Module<T>>::make_transfer_with_event(
        //         &asset_in,
        //         &sender,
        //         &Self::dex_account_id(),
        //         asset_in_amount,
        //     )?;
        //     // transfer `assets_out` to the receiver
        //     <balances::Module<T>>::make_transfer_with_event(
        //         &asset_out,
        //         &Self::dex_account_id(),
        //         &receiver,
        //         asset_out_amount,
        //     )?;

        //     <Exchanges<T>>::mutate(asset_in, |exchange| {
        //         exchange.update_pools(new_first_asset_pool_from, new_second_asset_pool_from)
        //     });
        //     <Exchanges<T>>::mutate(asset_out, |exchange| {
        //         exchange.update_pools(new_first_asset_pool_to, new_second_asset_pool_to)
        //     });

        //     Self::deposit_event(RawEvent::Exchanged(
        //         asset_in,
        //         asset_in_amount,
        //         asset_out,
        //         asset_out_amount,
        //         sender,
        //     ));
        //     Ok(())
        // }

        #[weight = 10_000]
        pub fn invest_liquidity(origin, first_asset_id: T::AssetId, second_asset_id: T::AssetId, shares: BalanceOf<T>) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            let (first_asset_id, second_asset_id, _) =
                Self::adjust_assets_order(first_asset_id, second_asset_id);

            let (first_asset_cost, second_asset_cost) = Self::ensure_exchange_exists(first_asset_id, second_asset_id)?.calculate_costs(shares);
            Self::ensure_sufficient_balances(&sender, first_asset_id, first_asset_cost, second_asset_id, second_asset_cost)?;

            //
            // == MUTATION SAFE ==
            //

            Self::slash_assets(&sender, first_asset_id, first_asset_cost, second_asset_id, second_asset_cost);


            <Exchanges<T>>::mutate(first_asset_id, second_asset_id, |exchange| {
                exchange.invest(first_asset_cost, second_asset_cost, shares, &sender)
            });

            Self::deposit_event(RawEvent::Invested(sender, first_asset_id, second_asset_id, shares));
            Ok(())
        }

        #[weight = 10_000]
        pub fn divest_liquidity(
            origin,
            first_asset_id: T::AssetId,
            second_asset_id: T::AssetId,
            shares_burned:  BalanceOf<T>,
            min_first_asset_received: BalanceOf<T>,
            min_second_asset_received: BalanceOf<T>
        ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            let (first_asset_id, second_asset_id, _) = Self::adjust_assets_order(first_asset_id, second_asset_id);

            let exchange = Self::ensure_exchange_exists(first_asset_id, second_asset_id)?;
            exchange.ensure_burned_shares(&sender, shares_burned)?;

            let (first_asset_cost, second_asset_cost) = exchange.calculate_costs(shares_burned);
            Self::ensure_divest_expectations(first_asset_cost, second_asset_cost, min_first_asset_received, min_second_asset_received)?;

            // Avoid overflow risks
            Self::ensure_can_hold_balances(&sender, first_asset_id, first_asset_cost, second_asset_id, second_asset_cost)?;

            //
            // == MUTATION SAFE ==
            //

            Self::mint_assets(&sender, first_asset_id, first_asset_cost, second_asset_id, second_asset_cost);

            <Exchanges<T>>::mutate(first_asset_id, second_asset_id, |exchange| {
                exchange.divest(first_asset_cost, second_asset_cost, shares_burned, &sender)
            });

            Self::deposit_event(RawEvent::Divested(sender, first_asset_id, second_asset_id, shares_burned));
            Ok(())
        }
    }
}

impl<T: Trait> Module<T> {
    pub fn ensure_valid_exchange(
        asset_in: T::AssetId,
        asset_out: T::AssetId,
    ) -> dispatch::DispatchResult {
        ensure!(asset_in != asset_out, Error::<T>::InvalidExchange);
        Ok(())
    }

    pub fn slash_assets(
        from: &T::AccountId,
        first_asset_id: T::AssetId,
        first_asset_amount: BalanceOf<T>,
        second_asset_id: T::AssetId,
        second_asset_amount: BalanceOf<T>,
    ) {
        Self::slash_asset(from, first_asset_id, first_asset_amount);
        Self::slash_asset(from, second_asset_id, second_asset_amount);
    }

    pub fn slash_asset(from: &T::AccountId, asset_id: T::AssetId, asset_amount: BalanceOf<T>) {
        // TODO
        // Refactor, when we`ll have native support for multiple currencies.
        if asset_id == T::KSMAssetId::get() {
            let _ = T::Currency::slash(from, asset_amount);
        } else {
            <AssetBalances<T>>::mutate(from, asset_id, |total_asset_amount| {
                *total_asset_amount -= asset_amount
            });
        }
    }

    pub fn mint_assets(
        to: &T::AccountId,
        first_asset_id: T::AssetId,
        first_asset_amount: BalanceOf<T>,
        second_asset_id: T::AssetId,
        second_asset_amount: BalanceOf<T>,
    ) {
        Self::mint_asset(to, first_asset_id, first_asset_amount);
        Self::mint_asset(to, second_asset_id, second_asset_amount);
    }

    pub fn mint_asset(to: &T::AccountId, asset_id: T::AssetId, asset_amount: BalanceOf<T>) {
        // TODO
        // Refactor, when we`ll have native support for multiple currencies.
        if asset_id == T::KSMAssetId::get() {
            T::Currency::deposit_creating(to, asset_amount);
        } else {
            <AssetBalances<T>>::mutate(to, asset_id, |asset_total_amount| {
                *asset_total_amount += asset_amount;
            });
        }
    }

    pub fn ensure_exchange_exists(
        first_asset_id: T::AssetId,
        second_asset_id: T::AssetId,
    ) -> Result<Exchange<T>, Error<T>> {
        let exchange = Self::exchanges(first_asset_id, second_asset_id);

        ensure!(
            exchange.invariant > BalanceOf::<T>::zero(),
            Error::<T>::ExchangeNotExists
        );
        Ok(exchange)
    }

    pub fn adjust_assets_amount_order(
        first_asset_id: T::AssetId,
        first_asset_amount: BalanceOf<T>,
        second_asset_id: T::AssetId,
        second_asset_amount: BalanceOf<T>,
    ) -> (T::AssetId, BalanceOf<T>, T::AssetId, BalanceOf<T>) {
        if first_asset_id > second_asset_id {
            (
                second_asset_id,
                second_asset_amount,
                first_asset_id,
                first_asset_amount,
            )
        } else {
            (
                first_asset_id,
                first_asset_amount,
                second_asset_id,
                second_asset_amount,
            )
        }
    }

    pub fn adjust_assets_order(
        first_asset_id: T::AssetId,
        second_asset_id: T::AssetId,
    ) -> (T::AssetId, T::AssetId, bool) {
        if first_asset_id > second_asset_id {
            (second_asset_id, first_asset_id, true)
        } else {
            (first_asset_id, second_asset_id, false)
        }
    }

    pub fn ensure_exchange_not_exists(
        first_asset_id: T::AssetId,
        second_asset_id: T::AssetId,
    ) -> dispatch::DispatchResult {
        let first_exchange = Self::exchanges(first_asset_id, second_asset_id);

        ensure!(
            first_exchange.invariant == BalanceOf::<T>::zero(),
            Error::<T>::ExchangeAlreadyExists
        );
        Ok(())
    }

    pub fn ensure_sufficient_balances(
        sender: &T::AccountId,
        asset_in: T::AssetId,
        asset_in_amount: BalanceOf<T>,
        asset_out: T::AssetId,
        asset_out_amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        Self::ensure_sufficient_balance(sender, asset_in, asset_in_amount)?;
        Self::ensure_sufficient_balance(sender, asset_out, asset_out_amount)
    }

    pub fn ensure_sufficient_balance(
        from: &T::AccountId,
        asset_id: T::AssetId,
        amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        match asset_id {
            // Here we also can add other currencies, with native dex parachain support.
            asset_id if asset_id == T::KSMAssetId::get() => {
                let new_balance = T::Currency::free_balance(from)
                    .checked_sub(&amount)
                    .ok_or(Error::<T>::InsufficientKsmBalance)?;

                T::Currency::ensure_can_withdraw(
                    from,
                    amount,
                    WithdrawReason::Transfer.into(),
                    new_balance,
                )?;
                Ok(())
            }
            asset_id if Self::asset_balances(from, asset_id) >= amount => Ok(()),
            _ => Err(Error::<T>::InsufficientOtherAssetBalance.into()),
        }
    }

    // Avoid overflow risks
    pub fn ensure_can_hold_balance(
        who: &T::AccountId,
        asset_id: T::AssetId,
        amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        if asset_id == T::KSMAssetId::get() {
            T::Currency::free_balance(who)
                .checked_add(&amount)
                .ok_or(Error::<T>::OverflowOccured)?;
        } else {
            Self::asset_balances(who, asset_id)
                .checked_add(&amount)
                .ok_or(Error::<T>::OverflowOccured)?;
        }
        Ok(())
    }

    // Avoid overflow risks
    pub fn ensure_can_hold_balances(
        who: &T::AccountId,
        first_asset_id: T::AssetId,
        first_asset_amount: BalanceOf<T>,
        second_asset_id: T::AssetId,
        second_asset_amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        Self::ensure_can_hold_balance(who, first_asset_id, first_asset_amount)?;
        Self::ensure_can_hold_balance(who, second_asset_id, second_asset_amount)
    }

    pub fn ensure_divest_expectations(
        first_asset_cost: BalanceOf<T>,
        second_asset_cost: BalanceOf<T>,
        min_first_asset_received: BalanceOf<T>,
        min_second_asset_received: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        ensure!(
            first_asset_cost >= min_first_asset_received,
            Error::<T>::KsmAmountBelowExpectation
        );
        ensure!(
            second_asset_cost >= min_second_asset_received,
            Error::<T>::AssetAmountBelowExpectation
        );
        Ok(())
    }
}
