"""Opt-in real GPU generated-media suites; never desktop/device capture.
Set REPLAYX_NATIVE_NVIDIA_TEST=1 or REPLAYX_NATIVE_AMD_TEST=1 only with approval.
Every run uses and identifies actual hardware. No AMD hardware was tested here.
"""
import os,unittest
import test_native_reliability as base
@unittest.skipUnless(os.environ.get('REPLAYX_NATIVE_NVIDIA_TEST')=='1','Opt-in actual NVIDIA hardware encoder')
class NvidiaReliability(base.NativeReliability):
    vendor='10de'
@unittest.skipUnless(os.environ.get('REPLAYX_NATIVE_AMD_TEST')=='1','Radeon hardware required; unverified on this machine')
class AmdReliability(base.NativeReliability):
    vendor='1002'
