// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.17;

import "forge-std/Script.sol";

import { BaseMultiChainDeployer } from "./BaseMultiChainDeployer.s.sol";
import { DeployContracts } from "./DeployContracts.s.soL";


contract DeployCatalyst is BaseMultiChainDeployer, DeployContracts {

    function deploy() iter_chains(chain_list) broadcast external {
        address admin = vm.envAddress("CATALYST_ADDRESS");
        deployAllContracts(admin);
    }
    function deploy_legacy() iter_chains(chain_list_legacy) broadcast external {
        address admin = vm.envAddress("CATALYST_ADDRESS");
        deployAllContracts(admin);
    }

    function fund_address(address toFund) iter_chains(chain_list) broadcast external {
        fund(toFund, 0.1*10**18);
    }

    function fund_address_legacy(address toFund) iter_chains(chain_list_legacy) broadcast external {
        fund(toFund, 0.1*10**18);
    }
}

