// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

interface IERC20Minimal {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address to, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
}

interface IV2PairMinimal {
    function token0() external view returns (address);
    function token1() external view returns (address);
    function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 timestamp);
    function swap(uint256 amount0Out, uint256 amount1Out, address to, bytes calldata data) external;
}

contract V2ArbitrageExecutor {
    struct V2SwapStep {
        address pair;
        address tokenIn;
        address tokenOut;
        uint256 minAmountOut;
    }

    address public immutable owner;
    mapping(address => bool) public executors;
    mapping(address => bool) public allowedPairs;
    mapping(address => bool) public allowedTokens;
    uint256 private locked = 1;

    error Unauthorized();
    error InvalidStepCount();
    error InvalidRoute();
    error NotAllowed();
    error DeadlineExpired();
    error MinimumOutput();
    error MinimumProfit();
    error TransferFailed();
    error ReentrantCall();

    constructor(address initialOwner) {
        if (initialOwner == address(0)) revert Unauthorized();
        owner = initialOwner;
    }

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    modifier onlyExecutor() {
        if (!executors[msg.sender]) revert Unauthorized();
        _;
    }

    modifier nonReentrant() {
        if (locked != 1) revert ReentrantCall();
        locked = 2;
        _;
        locked = 1;
    }

    function setExecutor(address account, bool allowed) external onlyOwner {
        executors[account] = allowed;
    }

    function setPair(address pair, bool allowed) external onlyOwner {
        allowedPairs[pair] = allowed;
    }

    function setToken(address token, bool allowed) external onlyOwner {
        allowedTokens[token] = allowed;
    }

    function executeV2Arbitrage(
        address anchorToken,
        uint256 amountIn,
        uint256 minProfit,
        uint256 deadline,
        V2SwapStep[] calldata steps
    ) external onlyExecutor nonReentrant {
        if (steps.length < 2 || steps.length > 3) revert InvalidStepCount();
        if (block.timestamp > deadline) revert DeadlineExpired();
        if (!allowedTokens[anchorToken] || steps[0].tokenIn != anchorToken) revert InvalidRoute();
        if (steps[steps.length - 1].tokenOut != anchorToken) revert InvalidRoute();

        for (uint256 i; i < steps.length; ++i) {
            V2SwapStep calldata step = steps[i];
            if (!allowedPairs[step.pair] || !allowedTokens[step.tokenIn] || !allowedTokens[step.tokenOut]) {
                revert NotAllowed();
            }
            if (i > 0 && steps[i - 1].tokenOut != step.tokenIn) revert InvalidRoute();
            for (uint256 j; j < i; ++j) {
                if (steps[j].pair == step.pair) revert InvalidRoute();
            }
            IV2PairMinimal pair = IV2PairMinimal(step.pair);
            address token0 = pair.token0();
            address token1 = pair.token1();
            if (!((token0 == step.tokenIn && token1 == step.tokenOut)
                || (token1 == step.tokenIn && token0 == step.tokenOut))) revert InvalidRoute();
        }

        uint256 initialBalance = IERC20Minimal(anchorToken).balanceOf(address(this));
        _safeTransferFrom(anchorToken, msg.sender, steps[0].pair, amountIn);
        uint256 currentAmount = amountIn;

        for (uint256 i; i < steps.length; ++i) {
            V2SwapStep calldata step = steps[i];
            IV2PairMinimal pair = IV2PairMinimal(step.pair);
            (uint112 reserve0, uint112 reserve1,) = pair.getReserves();
            bool zeroForOne = pair.token0() == step.tokenIn;
            uint256 reserveIn = zeroForOne ? reserve0 : reserve1;
            uint256 reserveOut = zeroForOne ? reserve1 : reserve0;
            uint256 actualInput = IERC20Minimal(step.tokenIn).balanceOf(step.pair) - reserveIn;
            if (actualInput < currentAmount) revert InvalidRoute();
            currentAmount = _getAmountOut(actualInput, reserveIn, reserveOut);
            if (currentAmount < step.minAmountOut) revert MinimumOutput();
            address recipient = i + 1 == steps.length ? address(this) : steps[i + 1].pair;
            pair.swap(zeroForOne ? 0 : currentAmount, zeroForOne ? currentAmount : 0, recipient, "");
        }

        uint256 finalBalance = IERC20Minimal(anchorToken).balanceOf(address(this));
        if (finalBalance < initialBalance + amountIn + minProfit) revert MinimumProfit();
        _safeTransfer(anchorToken, msg.sender, finalBalance - initialBalance);
    }

    function _getAmountOut(uint256 amountIn, uint256 reserveIn, uint256 reserveOut)
        private pure returns (uint256)
    {
        uint256 amountInWithFee = amountIn * 997;
        return amountInWithFee * reserveOut / (reserveIn * 1000 + amountInWithFee);
    }

    function _safeTransfer(address token, address to, uint256 amount) private {
        (bool success, bytes memory result) = token.call(
            abi.encodeCall(IERC20Minimal.transfer, (to, amount))
        );
        if (!success || (result.length != 0 && !abi.decode(result, (bool)))) revert TransferFailed();
    }

    function _safeTransferFrom(address token, address from, address to, uint256 amount) private {
        (bool success, bytes memory result) = token.call(
            abi.encodeCall(IERC20Minimal.transferFrom, (from, to, amount))
        );
        if (!success || (result.length != 0 && !abi.decode(result, (bool)))) revert TransferFailed();
    }
}
