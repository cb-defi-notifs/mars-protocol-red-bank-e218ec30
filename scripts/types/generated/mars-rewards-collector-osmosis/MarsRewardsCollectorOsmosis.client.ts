// @ts-nocheck
/**
 * This file was automatically generated by @cosmwasm/ts-codegen@0.30.0.
 * DO NOT MODIFY IT BY HAND. Instead, modify the source JSONSchema file,
 * and run the @cosmwasm/ts-codegen generate command to regenerate this file.
 */

import { CosmWasmClient, SigningCosmWasmClient, ExecuteResult } from '@cosmjs/cosmwasm-stargate'
import { Coin, StdFee } from '@cosmjs/amino'
import {
  Decimal,
  InstantiateMsg,
  ExecuteMsg,
  OwnerUpdate,
  OsmosisRoute,
  Uint128,
  UpdateConfig,
  SwapAmountInRoute,
  QueryMsg,
  ConfigResponse,
  RouteResponseForString,
  ArrayOfRouteResponseForString,
} from './MarsRewardsCollectorOsmosis.types'
export interface MarsRewardsCollectorOsmosisReadOnlyInterface {
  contractAddress: string
  config: () => Promise<ConfigResponse>
  route: ({
    denomIn,
    denomOut,
  }: {
    denomIn: string
    denomOut: string
  }) => Promise<RouteResponseForString>
  routes: ({
    limit,
    startAfter,
  }: {
    limit?: number
    startAfter?: string[][]
  }) => Promise<ArrayOfRouteResponseForString>
}
export class MarsRewardsCollectorOsmosisQueryClient
  implements MarsRewardsCollectorOsmosisReadOnlyInterface
{
  client: CosmWasmClient
  contractAddress: string

  constructor(client: CosmWasmClient, contractAddress: string) {
    this.client = client
    this.contractAddress = contractAddress
    this.config = this.config.bind(this)
    this.route = this.route.bind(this)
    this.routes = this.routes.bind(this)
  }

  config = async (): Promise<ConfigResponse> => {
    return this.client.queryContractSmart(this.contractAddress, {
      config: {},
    })
  }
  route = async ({
    denomIn,
    denomOut,
  }: {
    denomIn: string
    denomOut: string
  }): Promise<RouteResponseForString> => {
    return this.client.queryContractSmart(this.contractAddress, {
      route: {
        denom_in: denomIn,
        denom_out: denomOut,
      },
    })
  }
  routes = async ({
    limit,
    startAfter,
  }: {
    limit?: number
    startAfter?: string[][]
  }): Promise<ArrayOfRouteResponseForString> => {
    return this.client.queryContractSmart(this.contractAddress, {
      routes: {
        limit,
        start_after: startAfter,
      },
    })
  }
}
export interface MarsRewardsCollectorOsmosisInterface
  extends MarsRewardsCollectorOsmosisReadOnlyInterface {
  contractAddress: string
  sender: string
  updateOwner: (
    ownerUpdate: OwnerUpdate,
    fee?: number | StdFee | 'auto',
    memo?: string,
    _funds?: Coin[],
  ) => Promise<ExecuteResult>
  updateConfig: (
    {
      newCfg,
    }: {
      newCfg: UpdateConfig
    },
    fee?: number | StdFee | 'auto',
    memo?: string,
    _funds?: Coin[],
  ) => Promise<ExecuteResult>
  setRoute: (
    {
      denomIn,
      denomOut,
      route,
    }: {
      denomIn: string
      denomOut: string
      route: OsmosisRoute
    },
    fee?: number | StdFee | 'auto',
    memo?: string,
    _funds?: Coin[],
  ) => Promise<ExecuteResult>
  withdrawFromRedBank: (
    {
      amount,
      denom,
    }: {
      amount?: Uint128
      denom: string
    },
    fee?: number | StdFee | 'auto',
    memo?: string,
    _funds?: Coin[],
  ) => Promise<ExecuteResult>
  distributeRewards: (
    {
      amount,
      denom,
    }: {
      amount?: Uint128
      denom: string
    },
    fee?: number | StdFee | 'auto',
    memo?: string,
    _funds?: Coin[],
  ) => Promise<ExecuteResult>
  swapAsset: (
    {
      amount,
      denom,
    }: {
      amount?: Uint128
      denom: string
    },
    fee?: number | StdFee | 'auto',
    memo?: string,
    _funds?: Coin[],
  ) => Promise<ExecuteResult>
  claimIncentiveRewards: (
    fee?: number | StdFee | 'auto',
    memo?: string,
    _funds?: Coin[],
  ) => Promise<ExecuteResult>
}
export class MarsRewardsCollectorOsmosisClient
  extends MarsRewardsCollectorOsmosisQueryClient
  implements MarsRewardsCollectorOsmosisInterface
{
  client: SigningCosmWasmClient
  sender: string
  contractAddress: string

  constructor(client: SigningCosmWasmClient, sender: string, contractAddress: string) {
    super(client, contractAddress)
    this.client = client
    this.sender = sender
    this.contractAddress = contractAddress
    this.updateOwner = this.updateOwner.bind(this)
    this.updateConfig = this.updateConfig.bind(this)
    this.setRoute = this.setRoute.bind(this)
    this.withdrawFromRedBank = this.withdrawFromRedBank.bind(this)
    this.distributeRewards = this.distributeRewards.bind(this)
    this.swapAsset = this.swapAsset.bind(this)
    this.claimIncentiveRewards = this.claimIncentiveRewards.bind(this)
  }

  updateOwner = async (
    ownerUpdate: OwnerUpdate,
    fee: number | StdFee | 'auto' = 'auto',
    memo?: string,
    _funds?: Coin[],
  ): Promise<ExecuteResult> => {
    return await this.client.execute(
      this.sender,
      this.contractAddress,
      {
        update_owner: ownerUpdate,
      },
      fee,
      memo,
      _funds,
    )
  }
  updateConfig = async (
    {
      newCfg,
    }: {
      newCfg: UpdateConfig
    },
    fee: number | StdFee | 'auto' = 'auto',
    memo?: string,
    _funds?: Coin[],
  ): Promise<ExecuteResult> => {
    return await this.client.execute(
      this.sender,
      this.contractAddress,
      {
        update_config: {
          new_cfg: newCfg,
        },
      },
      fee,
      memo,
      _funds,
    )
  }
  setRoute = async (
    {
      denomIn,
      denomOut,
      route,
    }: {
      denomIn: string
      denomOut: string
      route: OsmosisRoute
    },
    fee: number | StdFee | 'auto' = 'auto',
    memo?: string,
    _funds?: Coin[],
  ): Promise<ExecuteResult> => {
    return await this.client.execute(
      this.sender,
      this.contractAddress,
      {
        set_route: {
          denom_in: denomIn,
          denom_out: denomOut,
          route,
        },
      },
      fee,
      memo,
      _funds,
    )
  }
  withdrawFromRedBank = async (
    {
      amount,
      denom,
    }: {
      amount?: Uint128
      denom: string
    },
    fee: number | StdFee | 'auto' = 'auto',
    memo?: string,
    _funds?: Coin[],
  ): Promise<ExecuteResult> => {
    return await this.client.execute(
      this.sender,
      this.contractAddress,
      {
        withdraw_from_red_bank: {
          amount,
          denom,
        },
      },
      fee,
      memo,
      _funds,
    )
  }
  distributeRewards = async (
    {
      amount,
      denom,
    }: {
      amount?: Uint128
      denom: string
    },
    fee: number | StdFee | 'auto' = 'auto',
    memo?: string,
    _funds?: Coin[],
  ): Promise<ExecuteResult> => {
    return await this.client.execute(
      this.sender,
      this.contractAddress,
      {
        distribute_rewards: {
          amount,
          denom,
        },
      },
      fee,
      memo,
      _funds,
    )
  }
  swapAsset = async (
    {
      amount,
      denom,
    }: {
      amount?: Uint128
      denom: string
    },
    fee: number | StdFee | 'auto' = 'auto',
    memo?: string,
    _funds?: Coin[],
  ): Promise<ExecuteResult> => {
    return await this.client.execute(
      this.sender,
      this.contractAddress,
      {
        swap_asset: {
          amount,
          denom,
        },
      },
      fee,
      memo,
      _funds,
    )
  }
  claimIncentiveRewards = async (
    fee: number | StdFee | 'auto' = 'auto',
    memo?: string,
    _funds?: Coin[],
  ): Promise<ExecuteResult> => {
    return await this.client.execute(
      this.sender,
      this.contractAddress,
      {
        claim_incentive_rewards: {},
      },
      fee,
      memo,
      _funds,
    )
  }
}
