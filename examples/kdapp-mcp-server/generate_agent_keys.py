#!/usr/bin/env python3
"""
Script to generate agent keys in the expected format for the TicTacToe coordinator
"""

import os
import json
import secrets
import hashlib

def generate_demo_key():
    """Generate a demo key in the same format as the coordinator"""
    priv = secrets.token_hex(32)
    # derive a pseudo-address for demo: sha256 of key, hex prefix
    addr = 'demo_' + hashlib.sha256(priv.encode()).hexdigest()[:40]
    return addr, priv

def create_agent_key(agent_name):
    """Create an agent key file"""
    # Create agent directory
    agent_dir = os.path.join('agent_keys', agent_name)
    os.makedirs(agent_dir, exist_ok=True)
    
    # Create key file
    keyfile = os.path.join(agent_dir, 'key.json')
    
    # Generate demo key
    addr, priv = generate_demo_key()
    
    # Save key data
    data = {
        'address': addr,
        'private_key_demo': priv
    }
    
    with open(keyfile, 'w') as f:
        json.dump(data, f)
    
    print(f"Created {agent_name} key:")
    print(f"  Address: {addr}")
    print(f"  Keyfile: {keyfile}")
    
    return addr, keyfile

if __name__ == '__main__':
    print("Generating agent keys for TicTacToe coordinator...")
    
    # Create agent keys directory
    os.makedirs('agent_keys', exist_ok=True)
    
    # Create keys for both agents
    agent1_addr, agent1_keyfile = create_agent_key('agent1')
    agent2_addr, agent2_keyfile = create_agent_key('agent2')
    
    print("\n✅ Agent keys generated successfully!")
    print(f"Agent 1: {agent1_addr}")
    print(f"Agent 2: {agent2_addr}")