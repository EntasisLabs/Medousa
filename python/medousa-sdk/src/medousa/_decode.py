from __future__ import annotations

from typing import TypeVar

from pydantic import BaseModel, ValidationError

from medousa.error import SdkError

T = TypeVar("T", bound=BaseModel)


def decode(model: type[T], value: object) -> T:
    try:
        return model.model_validate(value)
    except ValidationError as exc:
        raise SdkError(f"Failed to decode response: {exc}") from exc
