import pytest
import brownie


@pytest.fixture(scope="module")
def sample_pool(deploy_pool, tokens, deployer, amplification, max_pool_assets):
    yield deploy_pool(
        tokens          = tokens[:max_pool_assets],
        token_balances  = [10**8]*max_pool_assets,
        weights         = [1]*max_pool_assets,
        amp             = amplification,
        name            = "",
        symbol          = "",
        deployer        = deployer,
    )



# Main setup parametrized test **************************************************************************************************
# Test that all provided pool configs work correctly
def test_finish_setup(pool, deployer):

    assert not pool.ready()

    pool.finishSetup({"from": deployer})

    assert pool.ready()

    # TODO verify that all parameters are saved correctly on-chain



# Authority and state tests *****************************************************************************************************

def test_finish_setup_unauthorized(sample_pool, molly):

    with brownie.reverts():
        sample_pool.finishSetup({"from": molly})


def test_finish_setup_twice(sample_pool, deployer):

    sample_pool.finishSetup({"from": deployer})

    with brownie.reverts():
        sample_pool.finishSetup({"from": deployer})


def test_finish_setup_only_local(deploy_pool, tokens, deployer, amplification, max_pool_assets):

    sp = deploy_pool(
        tokens          = tokens[:max_pool_assets],
        token_balances  = [10**8]*max_pool_assets,
        weights         = [1]*max_pool_assets,
        amp             = amplification,
        name            = "",
        symbol          = "",
        deployer        = deployer,
        only_local      = True
    )

    sp.finishSetup({"from": deployer})

    assert sp.onlyLocal()


def test_finish_setup_not_only_local(deploy_pool, tokens, deployer, amplification, max_pool_assets):

    sp = deploy_pool(
        tokens          = tokens[:max_pool_assets],
        token_balances  = [10**8]*max_pool_assets,
        weights         = [1]*max_pool_assets,
        amp             = amplification,
        name            = "",
        symbol          = "",
        deployer        = deployer,
        only_local      = False
    )

    sp.finishSetup({"from": deployer})

    assert not sp.onlyLocal()

