/* The game's object layout, ids and key codes, from the binary's DWARF and symbol table. */
#ifndef FTLPAD_OFFSETS_H
#define FTLPAD_OFFSETS_H

/* The game's logical screen. */
#define GAME_W 1280

#define GAME_H 720

/* Game object layout, from the binary's DWARF. */
#define CAPP_GUI 16

#define CAPP_MAIN_MENU_OPEN 32

/* The Shift flag CApp::OnLoop samples once a frame and hands to the hotkeys. */
#define CAPP_SHIFT_HELD 9

#define CAPP_MODIFIER_X 14648
#define CAPP_MODIFIER_Y 14652
#define CAPP_MOUSE_MODIFIER_X 14668
#define CAPP_MOUSE_MODIFIER_Y 14672
#define CAPP_X_BAR 14692
#define CAPP_Y_BAR 14696
#define GUI_CREW_CONTROL 616
#define CREW_CONTROL_SIZE 552

/* The HUD's top row; with a screen open, buttons above it belong to the background. */
#define HUD_TOP_HEIGHT 90

/* The HUD's bottom bar starts here; the screens' own buttons end above it. */
#define HUD_BOTTOM_TOP 610

#define GUI_COMBAT_CONTROL 1416
#define GUI_SHIP_COMPLETE 6512
#define GUI_SHIP_POSITION 6656
#define GUI_STAR_MAP 6504
#define GUI_FTL_BUTTON 5928
#define FTL_BUTTON_SIZE 416
#define STARMAP_SIZE 3280

/* StarMap::closeButton stays active while hidden. */
#define FOCUS_OPEN 8

#define STARMAP_LOCATIONS 40
#define STARMAP_CURRENT_LOC 136
#define STARMAP_POSITION 160
#define STARMAP_TRANSLATION 184
#define STARMAP_CHOOSING_SECTOR 2232
#define STARMAP_SECTOR_OFFSET 3240
#define STARMAP_SECTOR_HITBOXES 3256
#define LOCATION_LOC 0
#define GUI_CHOICE_BOX 6688
#define CHOICE_RECTS 88
#define CHOICE_TEXT 48
#define CHOICE_OPEN_TIME 132
#define BUTTON_HITBOX 16
#define BUTTON_RENDER_OFF 34
#define BUTTON_ACTIVE 72
#define GUI_SYS_CONTROL 1168
#define SYSCTRL_BOXES 16
#define SYSBOX_SYSTEM 144
#define SYSTEM_TYPE 64
#define SYSTEM_POWER 80

/* ShipSystem::powerState is a pair: the power in use, then the capacity. */
#define SYSTEM_POWER_MAX (SYSTEM_POWER + sizeof(int))

#define COMBAT_WEAPON_CONTROL 32
#define COMBAT_DRONE_CONTROL 1056
#define WEAPONCTRL_ARMED 216
#define WEAPONCTRL_AUTOFIRE 224
#define ARMAMENT_BOXES 32
#define ARMAMENT_LOCATION 56
#define WEAPON_CONTROL_SIZE 1024
#define DRONE_CONTROL_SIZE 880
#define ARMAMENTBOX_ITEM 496
#define WEAPON_POWERED 242
#define WEAPON_BLUEPRINT 32
#define WEAPON_POWER 244
#define WEAPONBP_ICON 320
#define SYSTEM_ICON 96
#define TEXTURE_W 4
#define TEXTURE_H 8
#define DRONE_BLUEPRINT 32
#define DRONE_POWER 20
#define DRONEBP_ICON 168

/* Armament icons as the game's weapon boxes build them: prefix, blueprint icon, suffix. */
#define WEAPON_ICON_PREFIX "systemUI/weapbox_icon_W_"

#define DRONE_ICON_PREFIX "systemUI/weapbox_icon_D_"
#define ICON_SUFFIX ".png"
#define WEAPON_ICON_FALLBACK "icons/s_weapons_overlay.png"
#define DRONE_ICON_FALLBACK "icons/s_drones_overlay.png"
#define CREW_ANIM 192
#define BLUEPRINT_TITLE 16
#define BLUEPRINT_SHORT_TITLE 32
#define TEXTSTRING_LITERAL 8
#define TEXTLIB_LANGUAGE 0x60
#define DRONE_POWERED 16
#define SHIPMANAGER_CREW 192
#define CREW_ROOM 156
#define CREW_SHIP 396
#define PLAYER_SHIP_ID 0
#define CREW_DEAD 544
#define CREW_BLUEPRINT 576
#define CREWBP_NAME 120
#define CREWBP_RACE 8
#define CRYSTAL_RACE "crystal"

/* Crew hotkeys run F1 up to this many. */
#define CREW_HOTKEYS 8

#define GUI_PAUSED 12944
#define GUI_MENU_BOX 7232
#define MENU_BOX_SIZE 1136
#define GUI_OPTIONS_BOX 8592
#define OPTIONS_BOX_SIZE 4352
#define GUI_SHIP_SCREENS 13064
#define GUI_CREW_SCREEN 14552
#define GUI_EQUIP_SCREEN 15624
#define EQUIP_BOXES 472
#define CREWSCREEN_BOXES 184
#define EQUIPBOX_HITBOX 96
#define GUI_STORE_SCREENS 13448
#define TABBED_BUTTONS 32
#define TABBED_CURRENT 104
#define SHIP_DOORS 40
#define SHIP_AIRLOCKS 88
#define DOOR_X 592
#define DOOR_Y 596
#define DOOR_VERTICAL 600
#define DOOR_ROOM1 28
#define DOOR_ROOM2 32
#define DOOR_OPEN 36

/* Game system types with a power hotkey. */
#define SYS_SHIELDS 0

#define SYS_ENGINES 1
#define SYS_OXYGEN 2
#define SYS_WEAPONS 3
#define SYS_DRONES 4
#define SYS_MEDBAY 5
#define SYS_PILOT 6
#define SYS_SENSORS 7
#define SYS_DOORS 8
#define SYS_TELEPORTER 9
#define SYS_CLOAKING 10
#define SYS_ARTILLERY 11
#define SYS_BATTERY 12
#define SYS_CLONEBAY 13
#define SYS_MIND 14
#define SYS_HACKING 15
#define WEAPON_SLOTS 4
#define DRONE_SLOTS 3

/* CFPS::FPSControl's speed level, unused by the game itself. */
#define FPS_LEVEL 0x24

/* The step CFPS::OnLoop last computed, in the game's frame units. */
#define FPS_STEP 0x18

#define COMBAT_CURRENT_TARGET 1984
#define COMBAT_POSITION 2000
#define COMBAT_TARGET_POSITION 2016
#define CREW_SELECTED 8
#define COMPLETE_SHIP_MANAGER 16
#define SHIPMANAGER_SHIP 280
#define SHIP_ROOMS 16
#define ROOM_RECT 28
#define ROOM_ID 44
#define MOUSE_AIMING 16
#define MOUSE_TELEPORTING 20
#define MOUSE_MIND 24
#define MOUSE_HACKING 312
#define VECTOR_BEGIN 0
#define VECTOR_END 8

/* Key codes the game's key handlers take: SDL1 keysyms, ASCII for letters and digits. */
#define KEY_LSHIFT 0x130

#define KEY_F1 0x11a
#define KEY_F_COUNT 15
#define KEY_SPACE 0x20
#define KEY_ESCAPE 0x1b
#define KEY_ENTER 0x0d

#endif
