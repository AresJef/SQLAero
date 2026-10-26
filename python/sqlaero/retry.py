"""Explicit outer retry policies; SQL execution never retries implicitly.

Rules are AND within one rule and OR across rules. Only the current primary
exception matches. The caller owns replay safety, fresh connections and all
external side effects; a matching code does not establish transaction safety.
"""

from collections.abc import Callable
from functools import wraps
from typing import ParamSpec, TypeVar

from ._sqlaero import Retry as Retry, RetryAttempt as RetryAttempt, RetryRule as RetryRule

P = ParamSpec('P')
R = TypeVar('R')


def retry(
    *rules: RetryRule,
    max_attempts: int | None = 3,
    wait_interval: int | float = 1.0,
    wait_multiplier: int | float = 1.0,
    max_wait_interval: int | float = 10.0,
    wait_jitter: bool = False,
) -> Callable[[Callable[P, R]], Callable[P, R]]:
    """Retry an entire decorated call with its original parameters/return type.

    At least one explicit rule is required. max_attempts includes the first;
    None is unlimited. Waiting accepts finite nonnegative seconds (including
    zero), uses a multiplier >= 1 and always has a finite cap. Jitter samples
    zero through the capped base. Configuration is validated once here; each
    invocation gets independent native state. Final/unmatched/control-flow
    exceptions propagate unchanged, without accumulated retry notes/history.
    """
    configuration = Retry(
        *rules, max_attempts=max_attempts, wait_interval=wait_interval,
        wait_multiplier=wait_multiplier, max_wait_interval=max_wait_interval,
        wait_jitter=wait_jitter,
    )

    def decorate(function: Callable[P, R]) -> Callable[P, R]:
        if not callable(function):
            raise TypeError('retry can decorate only a callable')

        @wraps(function)
        def wrapped(*args: P.args, **kwargs: P.kwargs) -> R:
            # This thin boundary invokes user Python code. Native Retry owns
            # matching, counters and waiting; the Rust SQL core gets no callback.
            for attempt in configuration:
                with attempt:
                    return function(*args, **kwargs)
            raise RuntimeError('retry traversal ended without a result or propagated exception')

        return wrapped

    return decorate


__all__: list[str] = ['retry', 'Retry', 'RetryRule', 'RetryAttempt']
