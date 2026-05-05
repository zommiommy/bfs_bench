import pandas as pd
import matplotlib.pyplot as plt
import numpy as np
import seaborn as sns
import re

def parse_data_file(filepath):
    # Dictionary to store the parsed data
    data = {}
    
    with open(filepath, 'r') as file:
        for line in file:
            line = line.strip()
            if not line:
                continue
            
            # Extract parts using regex to handle the inconsistent spacing
            parts = [p.strip() for p in line.split("\t")]
            
            if len(parts) < 3:
                continue
            
            # Extract graph name from path
            max_depth = parts[0]
            graph_path = parts[1]
            graph_name = graph_path.split('/')[-1]

            # Extract data structure name
            ds_full_name = parts[2]
            
            # Assign a more readable name to the data structure
            if 'HashSet<usize>' in ds_full_name:
                ds_name = 'HashSet (Default)'
            elif 'FxHasher' in ds_full_name:
                ds_name = 'HashSet (FxHasher)'
            elif 'AHasher' in ds_full_name:
                ds_name = 'HashSet (AHasher)'
            elif 'WyHasherBuilder' in ds_full_name:
                ds_name = 'HashSet (WyHasher)'
            elif 'Xxh3DefaultBuilder' in ds_full_name:
                ds_name = 'HashSet (Xxh3)'
            elif 'RapidHasher' in ds_full_name:
                ds_name = 'HashSet (RapidHasher)'
            elif 'Vec<bool>' in ds_full_name:
                ds_name = 'Vec<bool>'
            elif 'BitVec' in ds_full_name:
                ds_name = 'BitVec'
            elif 'BTreeSet<usize>' in ds_full_name:
                ds_name = 'BTreeSet<usize>'
            elif 'RoaringTreemap' in ds_full_name:
                ds_name = 'RoaringTreemap'
            elif 'AdaptiveNodeSet' in ds_full_name:
                ds_name = 'AdaptiveNodeSet'
            else:
                raise ValueError(f"Unknown data structure: {ds_full_name}")
            
            # Get time (last element)
            time = int(parts[-1])
            
            if max_depth not in data:
                data[max_depth] = {}

            # Initialize the graph entry if not exists
            if graph_name not in data[max_depth]:
                data[max_depth][graph_name] = {}
            
            # Initialize data structure entry if not exists
            if ds_name not in data[max_depth][graph_name]:
                data[max_depth][graph_name][ds_name] = []
            
            # Add the timing
            data[max_depth][graph_name][ds_name].append(time)
    
    return {
        depth: {
            graph_name: {
                ds_name: np.array(times)
                for ds_name, times in ds_data.items()
            }
            for graph_name, ds_data in graphs_data.items()
        }
        for depth, graphs_data in data.items()
    }


def visualize_relative_performance(data, colors):
    """Create bar plots showing relative performance (ratio to best)"""
    # Set style
    plt.style.use('ggplot')
    sns.set_palette('muted')
    
    for max_depth, graphs_data in data.items():

        # Create a figure with subplots (adjust rows/cols as needed)
        n_graphs = len(graphs_data)
        n_cols = 3
        n_rows = (n_graphs + n_cols - 1) // n_cols
        
        fig, axes = plt.subplots(n_rows, n_cols, figsize=(15, 4 * n_rows))
        
        
        # Flatten the axes array for easier iteration
        axes = axes.flatten() if hasattr(axes, 'flatten') else axes
        
        for i, (graph_name, ds_data) in enumerate(graphs_data.items()):
            if i >= len(axes):
                break

            # Sort data structures by performance (ascending)
            sorted_items = sorted(ds_data.items(), key=lambda x: np.median(x[1]))
            ds_names = [item[0] for item in sorted_items]
            times = [item[1] for item in sorted_items]
            
            # Calculate relative times
            best_time = min(np.median(x) for x in times)
            relative_times = [np.median(time) / best_time for time in times]
            
            # Create bar plot
            ax = axes[i]
            bars = ax.bar(range(len(ds_names)), relative_times, color=[colors[name] for name in ds_names])
            
            # Add data structure names on x-axis
            ax.set_xticks(range(len(ds_names)))
            ax.set_xticklabels(ds_names, rotation=45, ha='right')
            
            # Add value labels on top of bars
            for j, bar in enumerate(bars):
                height = bar.get_height()
                ax.text(
                    bar.get_x() + bar.get_width()/2.,
                    height * 1.01,
                    f'{height:.2f}x',
                    ha='center', va='bottom', fontsize=8
                )
            
            # Add title and labels
            ax.set_title(f'Graph: {graph_name} MaxDepth={max_depth}', fontsize=10)
            ax.set_ylabel('Relative Time (lower is better)')
            
            # Remove top and right spines
            ax.spines['top'].set_visible(False)
            ax.spines['right'].set_visible(False)
            
            # Adjust y-axis to start from 0
            ax.set_ylim(bottom=0)
            
        # Remove any unused subplots
        for i in range(len(graphs_data), len(axes)):
            fig.delaxes(axes[i])
        
        plt.tight_layout()
        plt.savefig(f'bfs_relative_performance_{max_depth}.png', dpi=300, bbox_inches='tight')
        #plt.figure(fig.number)
        #plt.show()
        
    return fig

def create_summary_table(data, colors):
    """Create a summary table with averages across all graphs"""
    # Initialize dataframe
    summary_data = {}
    
    # For each data structure, calculate average performance ratio across all graphs
    for max_depth, graphs_data in data.items():
        for graph_name, ds_data in graphs_data.items():
            # Get the best time for this graph
            best_time = min(np.median(x) for x in ds_data.values())
            
            # Calculate relative performance for each data structure
            for ds_name, time in ds_data.items():
                ratio = np.median(time) / best_time
                
                if ds_name not in summary_data:
                    summary_data[ds_name] = []
                
                summary_data[ds_name].append(ratio)
        
        # Calculate average ratios
        avg_ratios = {ds: np.mean(ratios) for ds, ratios in summary_data.items()}
        
        # Create dataframe for summary
        df_summary = pd.DataFrame({
            'Data Structure': list(avg_ratios.keys()),
            'Average Relative Performance': list(avg_ratios.values())
        })
        
        # Sort by performance
        df_summary = df_summary.sort_values('Average Relative Performance')
        
        print("\nSummary of Average Relative Performance:")
        print(df_summary)
        
        # Create consolidated bar chart for average performance
        plt.figure(figsize=(10, 6))
        bars = plt.bar(df_summary['Data Structure'], df_summary['Average Relative Performance'],  color=[colors[name] for name in df_summary['Data Structure']])
        
        # Add labels on top of bars
        for bar in bars:
            height = bar.get_height()
            plt.text(
                bar.get_x() + bar.get_width()/2.,
                height * 1.01,
                f'{height:.2f}x',
                ha='center', va='bottom'
            )
        
        plt.title(f'Average Relative Performance Across All Graphs With MaxDepth={max_depth}')
        plt.ylabel('Average Relative Time (lower is better)')
        plt.xticks(rotation=45, ha='right')
        plt.tight_layout()
        plt.savefig(f'bfs_average_performance_{max_depth}.png', dpi=300, bbox_inches='tight')
        #plt.show()

def main():
    # Parse the data file
    data = parse_data_file('new.csv')
    
    palette = sns.color_palette('muted', len(data['1']["dblp-2010"]))
    colors = {name:palette[i] for i, name in enumerate(data['1']["dblp-2010"])}
    print(colors)
    
    # Create visualizations
    #fig_abs = visualize_absolute_performance(data)
    #plt.figure(fig_abs.number)
    #plt.show()
    
    visualize_relative_performance(data, colors)
    
    # Create summary table
    create_summary_table(data, colors)

if __name__ == "__main__":
    main()