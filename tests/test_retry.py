"""Explicit retry rule normalization, propagation, state isolation and typing."""

import gc
import inspect
import weakref
from enum import IntEnum
from typing import Any, NoReturn

import pytest
from sqlaero.errors import IntegrityError, OperationalError, SQLAeroError
from sqlaero.retry import Retry, RetryAttempt, RetryRule, retry
from sqlaero.typing import ErrorCodesInput, ErrorTypesInput, EscapeInput, PathInput


def test_public_aliases_and_immutable_normalized_conditions() -> None:
    assert ErrorCodesInput and ErrorTypesInput and EscapeInput and PathInput
    types: list[type[Exception]] = [OperationalError, IntegrityError]
    codes = [1205, 1213]
    rule = RetryRule(error_types=types, error_codes=codes)
    types.clear()
    codes.clear()
    assert rule.error_types == (OperationalError, IntegrityError)
    assert rule.error_codes == (1205, 1213)
    for name in ('error_types', 'error_codes'):
        with pytest.raises(AttributeError):
            setattr(rule, name, None)
    configuration = Retry(rule)
    assert configuration.rules == (rule,) and configuration.max_attempts == 3
    assert configuration.wait_interval == 1.0 and configuration.wait_multiplier == 1.0
    assert configuration.max_wait_interval == 10.0 and not configuration.wait_jitter
    for name in ('rules', 'max_attempts', 'wait_interval', 'wait_multiplier', 'max_wait_interval', 'wait_jitter'):
        with pytest.raises(AttributeError):
            setattr(configuration, name, None)


@pytest.mark.parametrize('settings', [
    {}, {'error_types': []}, {'error_codes': []}, {'error_types': (), 'error_codes': 1213},
    {'error_types': ValueError, 'error_codes': ()},
])
def test_absent_and_explicit_empty_conditions_are_rejected(settings: dict[str, Any]) -> None:
    with pytest.raises(ValueError):
        RetryRule(**settings)


@pytest.mark.parametrize('settings', [
    {'error_types': BaseException}, {'error_types': KeyboardInterrupt}, {'error_types': ValueError()},
    {'error_types': [ValueError, object]}, {'error_types': {'bad'}},
    {'error_codes': True}, {'error_codes': [1213, False]}, {'error_codes': '1213'},
])
def test_bad_rule_entries_are_rejected_at_creation(settings: dict[str, Any]) -> None:
    with pytest.raises(TypeError):
        RetryRule(**settings)


def attempts_for(error: Exception, *rules: RetryRule) -> tuple[int, Exception]:
    """Run matching against actual primary instances with no waiting."""
    count = 0
    try:
        for attempt in Retry(*rules, max_attempts=3, wait_interval=0):
            with attempt:
                count += 1
                raise error
    except Exception as caught:
        return count, caught
    raise AssertionError('the final original exception must propagate')


def test_and_within_or_across_rules_and_multiple_codes_types() -> None:
    constrained = RetryRule(error_types=(OperationalError, IntegrityError), error_codes=(1205, 1213))
    for value, expected in [(OperationalError('x', code=1213), 3), (IntegrityError('x', code=1205), 3),
                            (OperationalError('x', code=1062), 1), (ValueError('x'), 1)]:
        count, final = attempts_for(value, constrained)
        assert count == expected and final is value
    unconstrained_integrity = RetryRule(error_types=IntegrityError)
    value = IntegrityError('any integrity', code=999999)
    count, final = attempts_for(value, constrained, unconstrained_integrity)
    assert count == 3 and final is value
    count, final = attempts_for(value, unconstrained_integrity, unconstrained_integrity)
    assert count == 3 and final is value


def test_code_only_accepts_intenum_but_not_external_code_attributes_or_errno() -> None:
    class Code(IntEnum):
        LARGE = 2**300

    class External(ValueError):
        code = Code.LARGE

    rule = RetryRule(error_codes=Code.LARGE)
    normalized = rule.error_codes
    assert normalized is not None and normalized == (int(Code.LARGE),) and type(normalized[0]) is int
    for value, expected in [(SQLAeroError(code=Code.LARGE), 3), (External('x'), 1),
                            (OSError(1213, 'ordinary errno'), 1)]:
        assert attempts_for(value, rule)[0] == expected


@pytest.mark.parametrize('control', [KeyboardInterrupt(), SystemExit(2), GeneratorExit()])
def test_control_exceptions_are_never_suppressed(control: BaseException) -> None:
    count = 0
    with pytest.raises(type(control)) as caught:
        for attempt in Retry(RetryRule(error_types=Exception), wait_interval=0):
            with attempt:
                count += 1
                raise control
    assert caught.value is control and count == 1


def test_success_breaks_loop_and_reusing_configuration_restarts_state() -> None:
    configuration = Retry(RetryRule(error_types=ValueError), max_attempts=None, wait_interval=0)
    for _ in range(2):
        count = 0
        for attempt in configuration:
            with attempt as same:
                assert same is attempt
                count += 1
                if count < 4:
                    raise ValueError('again')
        assert count == 4
    left, right = iter(configuration), iter(configuration)
    with next(left):
        pass
    with next(right):
        pass
    with pytest.raises(StopIteration):
        next(left)
    with pytest.raises(StopIteration):
        next(right)


@pytest.mark.parametrize(('name', 'value', 'error'), [
    ('max_attempts', 0, ValueError), ('max_attempts', -1, ValueError),
    ('max_attempts', 2**64, ValueError), ('max_attempts', True, TypeError),
    ('max_attempts', 3.0, TypeError), ('max_attempts', '3', TypeError),
    ('wait_interval', -0.1, ValueError), ('wait_interval', float('nan'), ValueError),
    ('wait_interval', float('inf'), ValueError), ('wait_interval', True, TypeError),
    ('wait_interval', None, TypeError),
    pytest.param('wait_interval', 10**10000, ValueError, id='unrepresentable-large-integer'),
    ('wait_multiplier', 0.99, ValueError), ('wait_multiplier', float('nan'), ValueError),
    ('max_wait_interval', None, TypeError), ('max_wait_interval', -1, ValueError),
    ('max_wait_interval', float('inf'), ValueError), ('wait_jitter', 1, TypeError),
])
def test_retry_controls_are_checked_once_with_clear_parameter_names(name: str, value: object,
                                                                    error: type[Exception]) -> None:
    with pytest.raises(error, match=name):
        Retry(RetryRule(error_types=ValueError), **{name: value})  # type: ignore[arg-type]


def test_rules_required_attempt_creation_and_context_protocol() -> None:
    with pytest.raises(ValueError):
        Retry()
    with pytest.raises(TypeError):
        Retry(ValueError)  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        RetryAttempt()  # pyright: ignore[reportCallIssue] -- attempts come from Retry
    iterator = iter(Retry(RetryRule(error_types=ValueError), wait_interval=0))
    attempt = next(iterator)
    with pytest.raises(RuntimeError, match='previous'):
        next(iterator)
    with attempt:
        pass
    with pytest.raises(RuntimeError, match='only once'):
        attempt.__enter__()


def test_final_exception_identity_notes_chain_and_no_diagnostic_materialization() -> None:
    class Primary(SQLAeroError):
        @property
        def error_info(self) -> Any:
            raise AssertionError('matching must not materialize diagnostics')

    errors: list[SQLAeroError] = []

    @retry(RetryRule(error_codes=1213), wait_interval=0)
    def fails() -> None:
        current = Primary('original', code=1213, is_server_error=True)
        current.add_note('one existing note')
        errors.append(current)
        raise current

    with pytest.raises(Primary) as caught:
        fails()
    assert len(errors) == 3 and caught.value is errors[-1]
    assert caught.value.args == ('original',) and caught.value.__notes__ == ['one existing note']
    assert caught.value.__cause__ is None and caught.value.__context__ is None


def test_decorated_method_keeps_signature_return_annotations_and_call_binding() -> None:
    class Service:
        @retry(RetryRule(error_types=ValueError), max_attempts=2, wait_interval=0)
        def value(self, number: int, *, strict: bool = True) -> str:
            """Original application method."""
            return f'{number}:{strict}'

    assert Service().value(7, strict=False) == '7:False'
    assert str(inspect.signature(Service.value)) == '(self, number: int, *, strict: bool = True) -> str'
    assert Service.value.__name__ == 'value' and Service.value.__doc__ == 'Original application method.'


def test_completed_attempts_do_not_retain_previous_exceptions_or_history() -> None:
    class Transient(ValueError):
        pass

    references: list[weakref.ReferenceType[Transient]] = []

    def fails() -> NoReturn:
        current = Transient('temporary')
        references.append(weakref.ref(current))
        raise current

    for attempt in Retry(RetryRule(error_types=Transient), max_attempts=3, wait_interval=0):
        with attempt:
            if len(references) < 2:
                fails()
    gc.collect()
    assert all(reference() is None for reference in references)


def test_class_rule_configuration_reference_cycle_is_collectible() -> None:
    def create() -> weakref.ReferenceType[type[ValueError]]:
        class Failure(ValueError):
            pass
        rule = RetryRule(error_types=Failure)
        setattr(Failure, 'configuration', Retry(rule))
        return weakref.ref(Failure)
    reference = create()
    gc.collect()
    assert reference() is None


def test_zero_wait_and_small_capped_jitter_complete_without_extra_retries() -> None:
    for jitter in (False, True):
        count = 0
        for attempt in Retry(RetryRule(error_types=ValueError), max_attempts=3,
                             wait_interval=1, max_wait_interval=0.001, wait_jitter=jitter):
            with attempt:
                count += 1
                if count < 3:
                    raise ValueError('retry')
        assert count == 3
