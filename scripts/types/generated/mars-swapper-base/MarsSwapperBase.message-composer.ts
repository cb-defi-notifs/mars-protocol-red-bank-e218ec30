// @ts-nocheck
/**
 * This file was automatically generated by @cosmwasm/ts-codegen@0.23.0.
 * DO NOT MODIFY IT BY HAND. Instead, modify the source JSONSchema file,
 * and run the @cosmwasm/ts-codegen generate command to regenerate this file.
 */

import { MsgExecuteContractEncodeObject } from 'cosmwasm'
import { MsgExecuteContract } from 'cosmjs-types/cosmwasm/wasm/v1/tx'
import { toUtf8 } from '@cosmjs/encoding'
import {
  InstantiateMsg,
  ExecuteMsg,
  Uint128,
  Decimal,
  Addr,
  Empty,
  Coin,
  QueryMsg,
  AdminResponse,
  EstimateExactInSwapResponse,
  RouteResponseForEmpty,
  ArrayOfRouteResponseForEmpty,
} from './MarsSwapperBase.types'
export interface MarsSwapperBaseMessage {
  contractAddress: string
  sender: string
  updateAdmin: (
    {
      admin,
    }: {
      admin: string
    },
    funds?: Coin[],
  ) => MsgExecuteContractEncodeObject
  setRoute: (
    {
      denomIn,
      denomOut,
      route,
    }: {
      denomIn: string
      denomOut: string
      route: Empty
    },
    funds?: Coin[],
  ) => MsgExecuteContractEncodeObject
  swapExactIn: (
    {
      coinIn,
      denomOut,
      slippage,
    }: {
      coinIn: Coin
      denomOut: string
      slippage: Decimal
    },
    funds?: Coin[],
  ) => MsgExecuteContractEncodeObject
  transferResult: (
    {
      denomIn,
      denomOut,
      recipient,
    }: {
      denomIn: string
      denomOut: string
      recipient: Addr
    },
    funds?: Coin[],
  ) => MsgExecuteContractEncodeObject
}
export class MarsSwapperBaseMessageComposer implements MarsSwapperBaseMessage {
  sender: string
  contractAddress: string

  constructor(sender: string, contractAddress: string) {
    this.sender = sender
    this.contractAddress = contractAddress
    this.updateAdmin = this.updateAdmin.bind(this)
    this.setRoute = this.setRoute.bind(this)
    this.swapExactIn = this.swapExactIn.bind(this)
    this.transferResult = this.transferResult.bind(this)
  }

  updateAdmin = (
    {
      admin,
    }: {
      admin: string
    },
    funds?: Coin[],
  ): MsgExecuteContractEncodeObject => {
    return {
      typeUrl: '/cosmwasm.wasm.v1.MsgExecuteContract',
      value: MsgExecuteContract.fromPartial({
        sender: this.sender,
        contract: this.contractAddress,
        msg: toUtf8(
          JSON.stringify({
            update_admin: {
              admin,
            },
          }),
        ),
        funds,
      }),
    }
  }
  setRoute = (
    {
      denomIn,
      denomOut,
      route,
    }: {
      denomIn: string
      denomOut: string
      route: Empty
    },
    funds?: Coin[],
  ): MsgExecuteContractEncodeObject => {
    return {
      typeUrl: '/cosmwasm.wasm.v1.MsgExecuteContract',
      value: MsgExecuteContract.fromPartial({
        sender: this.sender,
        contract: this.contractAddress,
        msg: toUtf8(
          JSON.stringify({
            set_route: {
              denom_in: denomIn,
              denom_out: denomOut,
              route,
            },
          }),
        ),
        funds,
      }),
    }
  }
  swapExactIn = (
    {
      coinIn,
      denomOut,
      slippage,
    }: {
      coinIn: Coin
      denomOut: string
      slippage: Decimal
    },
    funds?: Coin[],
  ): MsgExecuteContractEncodeObject => {
    return {
      typeUrl: '/cosmwasm.wasm.v1.MsgExecuteContract',
      value: MsgExecuteContract.fromPartial({
        sender: this.sender,
        contract: this.contractAddress,
        msg: toUtf8(
          JSON.stringify({
            swap_exact_in: {
              coin_in: coinIn,
              denom_out: denomOut,
              slippage,
            },
          }),
        ),
        funds,
      }),
    }
  }
  transferResult = (
    {
      denomIn,
      denomOut,
      recipient,
    }: {
      denomIn: string
      denomOut: string
      recipient: Addr
    },
    funds?: Coin[],
  ): MsgExecuteContractEncodeObject => {
    return {
      typeUrl: '/cosmwasm.wasm.v1.MsgExecuteContract',
      value: MsgExecuteContract.fromPartial({
        sender: this.sender,
        contract: this.contractAddress,
        msg: toUtf8(
          JSON.stringify({
            transfer_result: {
              denom_in: denomIn,
              denom_out: denomOut,
              recipient,
            },
          }),
        ),
        funds,
      }),
    }
  }
}
