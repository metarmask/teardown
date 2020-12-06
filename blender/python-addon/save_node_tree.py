def save_node_tree_as_py(node_tree):
    s = ""
    s += f'stuff = []\n'
    s += f'tree = bpy.data.node_groups.new("{node_tree.name}", "ShaderNodeTree")\n'
    for input in node_tree.inputs:
        s += f'tree.inputs.new("{input.bl_socket_idname}", "{input.name}")\n'
    for output in node_tree.outputs:
        s += f'tree.outputs.new("{output.bl_socket_idname}", "{output.name}")\n'
    s += "\n"
    stuff = []
    for name, node in node_tree.nodes.items():
        stuff.append(node)
        s += f'node = tree.nodes.new("{node.bl_idname}")\n'
        s += f'stuff.append(node)\n'
        s += f'node.location = {node.location.to_tuple(3)}\n'
        props_props = {}
        for prop in dir(node):
            prop_props = {}
            props_props[prop] = prop_props
            path = None
            try:
                path = node.path_from_id(prop)
            except:
                prop_props["no_path"] = True
                pass
            if path != None:
                if(node.is_property_hidden(prop)):
                    prop_props["hidden"] = True
        struct = node.__class__.bl_rna
        for key, prop in struct.properties.items():
            if struct.base.properties.find(key) == -1 and not prop.is_hidden:
                if (prop.type != "POINTER" or (node.bl_idname == "ShaderNodeGroup" and key == "node_tree")) and (prop.type == "POINTER" or (node.bl_idname == "ShaderNodeMath" and key == "operation") or prop.default != getattr(node, key)):
                    value = getattr(node, key)
                    s += f'node.{key} = '
                    if prop.type == "STRING" or prop.type == "ENUM":
                        s += f'"{value}"'
                    elif prop.type == "POINTER": # (only node_tree from above)
                        s += f'bpy.data.node_groups["{node.node_tree.name}"]'
                    else:
                        s += str(value)
                    s += "\n"
        # if node.bl_idname == "NodeGroupInput" or node.bl_idname == "NodeGroupOutput":
            
            # for i, input in enumerate(node.inputs):
            #     if input.bl_idname != "NodeSocketVirtual":
            #         s += f'node.inputs.new("{input.type}", "{input.name}")\n'
            # for i, output in enumerate(node.outputs):
            #     if output.bl_idname != "NodeSocketVirtual":
            #         s += f'node.outputs.new("{output.type}", "{output.name}")\n'

        for key, socket in node.inputs.items():
            if socket.enabled and not socket.is_linked:
                try:
                    value = node.inputs[key].default_value
                    s += f'node.inputs[{node.inputs.find(key)}].default_value = '
                    if socket.type == "VECTOR":
                        s += f'({value.x}, {value.y}, {value.z})'
                    else:
                        s += f'{str(value)}'
                    s += "\n"
                except:
                    pass
        s += "\n"
    s += "\n"
    s += "links = tree.links\n"
    for key, link in node_tree.links.items():
        from_node = stuff.index(link.from_node)
        for i, socket in enumerate(link.from_node.outputs):
            if link.from_socket == socket:
                from_socket = i
        to_node = stuff.index(link.to_node)
        for i, socket in enumerate(link.to_node.inputs):
            if link.to_socket == socket:
                to_socket = i
        s += f'link = links.new(stuff[{from_node}].outputs[{from_socket}], stuff[{to_node}].inputs[{to_socket}])'
        s += f" # {link.from_node.name}[{link.from_socket.name}] -> {link.to_node.name}[{link.to_socket.name}]\n"
        #s += f'stuff[{from_node}].update()\n'
        #s += f'stuff[{to_node}].update()\n'
    return s
