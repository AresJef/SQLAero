"""Explicit synchronous retry policies with preserved callable signatures."""
from collections.abc import Callable
from typing import ParamSpec, TypeVar
from ._sqlaero import Retry as Retry, RetryAttempt as RetryAttempt, RetryRule as RetryRule

P = ParamSpec('P')
R = TypeVar('R')

def retry(*rules: RetryRule, max_attempts: int | None = 3,
          wait_interval: int | float = 1.0, wait_multiplier: int | float = 1.0,
          max_wait_interval: int | float = 10.0, wait_jitter: bool = False
          ) -> Callable[[Callable[P, R]], Callable[P, R]]:
    """Retry the entire explicit call; final errors propagate without history.

    Rules are AND internally/OR externally. Attempts include the first; None
    is unlimited. Delays are bounded, permit zero and support optional jitter.
    No SQL replay, reconnection or transaction-safety inference is performed.
    """
    ...
