"""Medousa Python SDK — HTTP client for the Medousa daemon."""

from medousa.client import MedousaClient
from medousa.error import CompatibilityError, SdkError
from medousa.sync import MedousaClientSync
from medousa.transport import HttpTransport, WorkshopTransport

__all__ = [
    "MedousaClient",
    "MedousaClientSync",
    "CompatibilityError",
    "SdkError",
    "HttpTransport",
    "WorkshopTransport",
]

__version__ = "0.1.0"
