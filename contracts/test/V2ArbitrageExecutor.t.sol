// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "../src/V2ArbitrageExecutor.sol";

contract MockToken is IERC20Minimal {
    mapping(address => uint256) public balanceOf;
    function mint(address to, uint256 amount) external { balanceOf[to] += amount; }
    function transfer(address to, uint256 amount) external returns (bool) {
        balanceOf[msg.sender] -= amount; balanceOf[to] += amount; return true;
    }
    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        balanceOf[from] -= amount; balanceOf[to] += amount; return true;
    }
}

contract MockPair is IV2PairMinimal {
    address public immutable token0;
    address public immutable token1;
    uint112 private reserve0;
    uint112 private reserve1;
    constructor(address a, address b, uint112 x, uint112 y) {
        token0 = a; token1 = b; reserve0 = x; reserve1 = y;
    }
    function getReserves() external view returns (uint112, uint112, uint32) {
        return (reserve0, reserve1, 0);
    }
    function swap(uint256 amount0Out, uint256 amount1Out, address to, bytes calldata) external {
        if (amount0Out > 0) IERC20Minimal(token0).transfer(to, amount0Out);
        if (amount1Out > 0) IERC20Minimal(token1).transfer(to, amount1Out);
    }
}

contract ExternalCaller {
    function callExecutor(V2ArbitrageExecutor executor, V2ArbitrageExecutor.V2SwapStep[] calldata steps) external {
        executor.executeV2Arbitrage(address(1), 1, 0, block.timestamp, steps);
    }
}

contract V2ArbitrageExecutorTest {
    V2ArbitrageExecutor private executor;

    constructor() {
        executor = new V2ArbitrageExecutor(address(this));
        executor.setExecutor(address(this), true);
    }

    function testThreeHopSuccess() external {
        MockToken a = new MockToken(); MockToken b = new MockToken(); MockToken c = new MockToken();
        MockPair p1 = new MockPair(address(a), address(b), 1_000_000, 1_000_000);
        MockPair p2 = new MockPair(address(b), address(c), 1_000_000, 1_000_000);
        MockPair p3 = new MockPair(address(c), address(a), 1_000_000, 1_100_000);
        _allow(address(a), address(b), address(c), address(p1), address(p2), address(p3));
        a.mint(address(this), 100_000); a.mint(address(p1), 1_000_000); b.mint(address(p1), 1_000_000);
        c.mint(address(p2), 1_000_000); a.mint(address(p3), 1_100_000);
        b.mint(address(p2), 1_000_000); c.mint(address(p3), 1_000_000);
        V2ArbitrageExecutor.V2SwapStep[] memory steps = new V2ArbitrageExecutor.V2SwapStep[](3);
        steps[0] = V2ArbitrageExecutor.V2SwapStep(address(p1), address(a), address(b), 1);
        steps[1] = V2ArbitrageExecutor.V2SwapStep(address(p2), address(b), address(c), 1);
        steps[2] = V2ArbitrageExecutor.V2SwapStep(address(p3), address(c), address(a), 1);
        uint256 beforeBalance = a.balanceOf(address(this));
        executor.executeV2Arbitrage(address(a), 10_000, 1, block.timestamp, steps);
        require(a.balanceOf(address(this)) > beforeBalance, "no profit");
    }

    function testTwoHopStillSucceeds() external {
        MockToken a = new MockToken(); MockToken b = new MockToken();
        MockPair p1 = new MockPair(address(a), address(b), 1_000_000, 1_000_000);
        MockPair p2 = new MockPair(address(b), address(a), 1_000_000, 1_100_000);
        executor.setToken(address(a), true); executor.setToken(address(b), true);
        executor.setPair(address(p1), true); executor.setPair(address(p2), true);
        a.mint(address(this), 100_000); a.mint(address(p1), 1_000_000);
        b.mint(address(p1), 1_000_000); b.mint(address(p2), 1_000_000); a.mint(address(p2), 1_100_000);
        V2ArbitrageExecutor.V2SwapStep[] memory steps = new V2ArbitrageExecutor.V2SwapStep[](2);
        steps[0] = V2ArbitrageExecutor.V2SwapStep(address(p1), address(a), address(b), 1);
        steps[1] = V2ArbitrageExecutor.V2SwapStep(address(p2), address(b), address(a), 1);
        uint256 beforeBalance = a.balanceOf(address(this));
        executor.executeV2Arbitrage(address(a), 10_000, 1, block.timestamp, steps);
        require(a.balanceOf(address(this)) > beforeBalance, "no profit");
    }

    function testRejectsOneAndFourSteps() external {
        V2ArbitrageExecutor.V2SwapStep[] memory one = new V2ArbitrageExecutor.V2SwapStep[](1);
        try executor.executeV2Arbitrage(address(1), 1, 0, block.timestamp, one) { revert("accepted one"); } catch {}
        V2ArbitrageExecutor.V2SwapStep[] memory four = new V2ArbitrageExecutor.V2SwapStep[](4);
        try executor.executeV2Arbitrage(address(1), 1, 0, block.timestamp, four) { revert("accepted four"); } catch {}
    }

    function testRejectsNonExecutor() external {
        ExternalCaller caller = new ExternalCaller();
        V2ArbitrageExecutor.V2SwapStep[] memory steps = new V2ArbitrageExecutor.V2SwapStep[](2);
        try caller.callExecutor(executor, steps) { revert("accepted caller"); } catch {}
    }

    function _allow(address a, address b, address c, address p1, address p2, address p3) private {
        executor.setToken(a, true); executor.setToken(b, true); executor.setToken(c, true);
        executor.setPair(p1, true); executor.setPair(p2, true); executor.setPair(p3, true);
    }
}
