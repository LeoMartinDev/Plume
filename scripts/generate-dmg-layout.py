#!/usr/bin/env python3
"""Regenerate the Finder layout (development only: pip install ds_store==1.3.3).

Release packaging copies this portable template; it needs neither Python nor
Finder automation. The template contains no machine-specific paths or aliases.
"""
from pathlib import Path
from ds_store import DSStore

output = Path(__file__).resolve().parents[1] / "crates/plume-app/packaging/dmg-layout.ds-store"
with DSStore.open(str(output), "w+") as store:
    store["."]["bwsp"] = {
        "WindowBounds": "{{200, 200}, {540, 320}}",
        "ShowToolbar": False,
        "ShowStatusBar": False,
        "ShowSidebar": False,
        "ShowTabView": False,
        "ShowPathbar": False,
        "ContainerShowSidebar": False,
        "SidebarWidth": 0,
    }
    store["."]["icvp"] = {
        "viewOptionsVersion": 1,
        "backgroundType": 1,
        "backgroundColorRed": 1.0,
        "backgroundColorGreen": 1.0,
        "backgroundColorBlue": 1.0,
        "arrangeBy": "none",
        "gridOffsetX": 0.0,
        "gridOffsetY": 0.0,
        "gridSpacing": 100.0,
        "iconSize": 96.0,
        "textSize": 14.0,
        "labelOnBottom": True,
        "showItemInfo": False,
        "showIconPreview": True,
        "scrollPositionX": 0.0,
        "scrollPositionY": 0.0,
    }
    store["."]["icvl"] = ("type", b"icnv")
    store["Plume.app"]["Iloc"] = (140, 130)
    store["Applications"]["Iloc"] = (400, 130)
print(output)
